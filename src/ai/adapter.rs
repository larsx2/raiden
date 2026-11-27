use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::v1::ConnectionTo;
use agent_client_protocol::v1::schema::{
    CancelNotification, CloseSessionRequest, ContentBlock, Implementation, InitializeRequest, Meta,
    NewSessionRequest, PromptRequest, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SessionId, SessionNotification, SessionUpdate, StopReason,
    SystemPrompt, SystemPromptMode, TextContent,
};
use agent_client_protocol::{self as sdk, Agent, Lines};
use futures::{SinkExt, TryStreamExt};
use raiden_core::ai::{Effort, Provider, Web};
use tokio::sync::{mpsc, oneshot};
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};

use super::process::{self, Group};

const REQUEST_LIMIT: Duration = Duration::from_secs(60);
pub const ANSWER_LIMIT: Duration = Duration::from_secs(300);
const STOP_LIMIT: Duration = Duration::from_secs(5);
const CLOSE_LIMIT: Duration = Duration::from_secs(5);
const LONGEST_LINE: usize = 16 * 1024 * 1024;

pub struct Launch {
    pub provider: Provider,
    pub effort: Effort,
    pub web: Web,
    pub program: PathBuf,
    pub env: Vec<(String, String)>,
    pub workdir: PathBuf,
    pub instructions: String,
}

#[derive(Clone)]
pub struct Session {
    pub id: SessionId,
    pub model: String,
    pub effort: Effort,
    pub web: Web,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    Answered(StopReason),
    Stopped,
    TimedOut,
}

pub struct Turn {
    pub ending: Ending,
    pub unresponsive: bool,
    pub searched: bool,
    pub sent: bool,
}

type Routes = Arc<Mutex<HashMap<SessionId, mpsc::UnboundedSender<SessionUpdate>>>>;

pub struct Adapter {
    pub provider: Provider,
    effort: Effort,
    web: Web,
    connection: ConnectionTo<Agent>,
    routes: Routes,
    workdir: PathBuf,
    instructions: String,
    spare: tokio::sync::Mutex<Option<Session>>,
    process: Mutex<Group>,
    _stop: oneshot::Sender<()>,
}

impl Adapter {
    pub async fn start(launch: Launch) -> Result<Self, String> {
        let name = launch.provider.through();
        let mut command = process::command(&launch.program, &launch.env, &launch.workdir);
        command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        let mut group = Group::spawn(&mut command)
            .map_err(|error| format!("Raiden could not start {name}'s adapter: {error}"))?;
        let stdin = group.child.stdin.take().ok_or("no stdin")?;
        let stdout = group.child.stdout.take().ok_or("no stdout")?;
        let routes: Routes = Arc::default();
        let (connection, stop) = connect(
            stdin,
            stdout,
            Arc::clone(&routes),
            launch.provider.adapter(),
        );
        let connection = connection
            .await
            .map_err(|_| format!("{name}'s adapter closed its connection"))?;
        let adapter = Self {
            provider: launch.provider,
            effort: launch.effort,
            web: launch.web,
            connection,
            routes,
            workdir: launch.workdir,
            instructions: launch.instructions,
            spare: tokio::sync::Mutex::new(None),
            process: Mutex::new(group),
            _stop: stop,
        };
        adapter
            .request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_info(Implementation::new("raiden", env!("CARGO_PKG_VERSION"))),
            )
            .await?;
        Ok(adapter)
    }

    pub fn serves(&self, effort: Effort) -> bool {
        !self.provider.effort_per_process() || self.effort == effort
    }

    pub fn searches(&self, web: Web) -> bool {
        !self.provider.effort_per_process() || self.web == web
    }

    pub fn running(&self) -> bool {
        self.process.lock().is_ok_and(|mut group| group.running())
    }

    async fn request<R: sdk::JsonRpcRequest>(&self, request: R) -> Result<R::Response, String> {
        let sent = self.connection.send_request(request).block_task();
        match tokio::time::timeout(REQUEST_LIMIT, sent).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(error)) => Err(self.explain(&error)),
            Err(_) => Err(format!(
                "{} did not answer within {} s",
                self.provider.through(),
                REQUEST_LIMIT.as_secs()
            )),
        }
    }

    fn explain(&self, error: &sdk::Error) -> String {
        if error.code == sdk::ErrorCode::AUTH_REQUIRED {
            return self.provider.signed_out_message();
        }
        if !self.running() {
            return format!("{} stopped unexpectedly", self.provider.through());
        }
        let details = error
            .data
            .as_ref()
            .and_then(|data| data.get("details"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|details| !details.is_empty());
        match details {
            Some(details) => format!("{}: {details}", error.message),
            None => error.message.clone(),
        }
    }

    fn fitted(&self, effort: Effort) -> Effort {
        if self.provider.effort_per_process() {
            self.effort
        } else {
            effort
        }
    }

    fn fitted_web(&self, web: Web) -> Web {
        if self.provider.effort_per_process() {
            self.web
        } else {
            web
        }
    }

    fn kept_off(&self, web: Web) -> Result<Web, String> {
        let fitted = self.fitted_web(web);
        if web == Web::Off && fitted != Web::Off {
            return Err(format!(
                "{} was started able to search the web, so Raiden did not ask it this",
                self.provider.through()
            ));
        }
        Ok(fitted)
    }

    async fn open(&self, web: Web, effort: Effort) -> Result<Session, String> {
        let effort = self.fitted(effort);
        let web = self.kept_off(web)?;
        let mut request = NewSessionRequest::new(&self.workdir);
        if let Some(meta) = meta(self.provider, web, effort) {
            request = request.meta(meta);
        }
        if self.provider == Provider::Claude {
            request = request.system_prompt(
                SystemPrompt::new(vec![ContentBlock::Text(TextContent::new(
                    self.instructions.clone(),
                ))])
                .mode(SystemPromptMode::Override),
            );
        }
        let response = self.request(request).await?;
        Ok(Session {
            model: self.provider.model().to_owned(),
            effort,
            web,
            id: response.session_id,
        })
    }

    pub async fn prepare(&self, effort: Effort) -> Result<Session, String> {
        let effort = self.fitted(effort);
        let mut spare = self.spare.lock().await;
        match spare.take() {
            Some(session) if session.effort == effort => {
                *spare = Some(session.clone());
                return Ok(session);
            }
            Some(stale) => self.close(&stale),
            None => {}
        }
        let session = self.open(Web::Searchable, effort).await?;
        *spare = Some(session.clone());
        Ok(session)
    }

    pub async fn take(&self, web: Web, effort: Effort) -> Result<Session, String> {
        let effort = self.fitted(effort);
        self.kept_off(web)?;
        if self.fitted_web(web) != self.fitted_web(Web::Searchable) {
            return self.open(web, effort).await;
        }
        let taken = self.spare.lock().await.take();
        match taken {
            Some(session) if session.effort == effort => Ok(session),
            Some(stale) => {
                self.close(&stale);
                self.open(web, effort).await
            }
            None => self.open(web, effort).await,
        }
    }

    pub async fn prompt(
        &self,
        session: &Session,
        text: &str,
        stop: &mut oneshot::Receiver<()>,
        mut on_heard: impl FnMut(Heard<'_>),
    ) -> Result<Turn, String> {
        if !matches!(stop.try_recv(), Err(oneshot::error::TryRecvError::Empty)) {
            return Ok(Turn {
                ending: Ending::Stopped,
                unresponsive: false,
                searched: false,
                sent: false,
            });
        }
        let (updates, mut updated) = mpsc::unbounded_channel();
        if let Ok(mut routes) = self.routes.lock() {
            routes.insert(session.id.clone(), updates);
        }
        let response = self
            .connection
            .send_request(PromptRequest::new(
                session.id.clone(),
                vec![ContentBlock::Text(TextContent::new(text))],
            ))
            .block_task();
        tokio::pin!(response);
        let deadline = tokio::time::sleep(ANSWER_LIMIT);
        tokio::pin!(deadline);
        let mut cut = None;
        let mut answer = Answer::default();
        let result = loop {
            tokio::select! {
                biased;
                result = &mut response => break Some(result),
                _ = &mut *stop, if cut.is_none() => {
                    cut = Some(Ending::Stopped);
                    self.cancel(session);
                    deadline.as_mut().reset(tokio::time::Instant::now() + STOP_LIMIT);
                }
                () = &mut deadline => {
                    if cut.is_some() {
                        break None;
                    }
                    cut = Some(Ending::TimedOut);
                    self.cancel(session);
                    deadline.as_mut().reset(tokio::time::Instant::now() + STOP_LIMIT);
                }
                Some(update) = updated.recv() => answer.hear(&update, &mut on_heard),
            }
        };
        while let Ok(update) = updated.try_recv() {
            answer.hear(&update, &mut on_heard);
        }
        if let Ok(mut routes) = self.routes.lock() {
            routes.remove(&session.id);
        }
        let Some(result) = result else {
            return Ok(Turn {
                ending: cut.unwrap_or(Ending::Stopped),
                unresponsive: true,
                searched: answer.used_tools,
                sent: true,
            });
        };
        let response = match result {
            Ok(response) => response,
            Err(_) if cut.is_some() => {
                return Ok(Turn {
                    ending: cut.unwrap_or(Ending::Stopped),
                    unresponsive: false,
                    searched: answer.used_tools,
                    sent: true,
                });
            }
            Err(error) => return Err(self.explain(&error)),
        };
        Ok(Turn {
            ending: cut.unwrap_or(Ending::Answered(response.stop_reason)),
            unresponsive: false,
            searched: answer.used_tools,
            sent: true,
        })
    }

    fn cancel(&self, session: &Session) {
        let _ = self
            .connection
            .send_notification(CancelNotification::new(session.id.clone()));
    }

    pub fn close(&self, session: &Session) {
        let closing = self
            .connection
            .send_request(CloseSessionRequest::new(session.id.clone()))
            .block_task();
        tokio::spawn(async move {
            let _ = tokio::time::timeout(CLOSE_LIMIT, closing).await;
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heard<'a> {
    Text(&'a str),
    Searching(&'a str),
}

#[derive(Default)]
struct Answer {
    spoke: bool,
    tail: String,
    searched: bool,
    used_tools: bool,
    query: String,
}

const PARAGRAPH: usize = 2;
const TAIL: usize = 16;

impl Answer {
    fn hear(&mut self, update: &SessionUpdate, on_heard: &mut impl FnMut(Heard<'_>)) {
        if let Some(text) = said(update) {
            let words = !text.trim().is_empty();
            if words && self.searched && self.spoke {
                let present = trailing_breaks(&self.tail) + leading_breaks(text);
                let missing = "\n".repeat(PARAGRAPH.saturating_sub(present));
                self.write(&missing, on_heard);
            }
            if words {
                self.searched = false;
                self.spoke = true;
            }
            self.write(text, on_heard);
        } else if let Some(query) = searching(update) {
            if matches!(update, SessionUpdate::ToolCall(_)) {
                self.searched = true;
                self.used_tools = true;
            }
            if matches!(update, SessionUpdate::ToolCall(_)) || query != self.query {
                self.query = query.to_owned();
                on_heard(Heard::Searching(query));
            }
        }
    }

    fn write(&mut self, text: &str, on_heard: &mut impl FnMut(Heard<'_>)) {
        if text.is_empty() {
            return;
        }
        self.tail.push_str(text);
        let extra = self.tail.chars().count().saturating_sub(TAIL);
        if extra > 0 {
            self.tail = self.tail.chars().skip(extra).collect();
        }
        on_heard(Heard::Text(text));
    }
}

fn searching(update: &SessionUpdate) -> Option<&str> {
    match update {
        SessionUpdate::ToolCall(call) => Some(query_of(call.raw_input.as_ref()).unwrap_or("")),
        SessionUpdate::ToolCallUpdate(update) => query_of(update.fields.raw_input.as_ref()),
        _ => None,
    }
}

fn query_of(input: Option<&serde_json::Value>) -> Option<&str> {
    input?.get("query")?.as_str()
}

fn trailing_breaks(text: &str) -> usize {
    text.trim_end_matches([' ', '\t'])
        .chars()
        .rev()
        .take_while(|c| *c == '\n')
        .count()
}

fn leading_breaks(text: &str) -> usize {
    text.trim_start_matches([' ', '\t'])
        .chars()
        .take_while(|c| *c == '\n')
        .count()
}

fn said(update: &SessionUpdate) -> Option<&str> {
    match update {
        SessionUpdate::AgentMessageChunk(chunk) => match &chunk.content {
            ContentBlock::Text(text) => Some(text.text.as_str()),
            _ => None,
        },
        _ => None,
    }
}

fn meta(provider: Provider, web: Web, effort: Effort) -> Option<Meta> {
    match provider {
        Provider::Claude => raiden_core::ai::claude_options(provider.model(), web, effort)
            .as_object()
            .cloned(),
        Provider::Chatgpt => None,
    }
}

fn connect(
    stdin: tokio::process::ChildStdin,
    stdout: tokio::process::ChildStdout,
    routes: Routes,
    tool: &'static str,
) -> (oneshot::Receiver<ConnectionTo<Agent>>, oneshot::Sender<()>) {
    let incoming = FramedRead::new(stdout, LinesCodec::new_with_max_length(LONGEST_LINE))
        .map_err(std::io::Error::other);
    let outgoing = SinkExt::<String>::sink_map_err(
        FramedWrite::new(stdin, LinesCodec::new()),
        std::io::Error::other,
    );
    let (ready, connection) = oneshot::channel();
    let (stop, stopped) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let result = sdk::Client
            .v1()
            .name("raiden")
            .on_receive_notification(
                async move |notification: SessionNotification, _| {
                    let route = routes
                        .lock()
                        .ok()
                        .and_then(|routes| routes.get(&notification.session_id).cloned());
                    if let Some(route) = route {
                        let _ = route.send(notification.update);
                    }
                    Ok(())
                },
                sdk::on_receive_notification!(),
            )
            .on_receive_notification(
                async move |_: sdk::UntypedMessage, _| Ok(()),
                sdk::on_receive_notification!(),
            )
            .on_receive_request(
                async move |_: RequestPermissionRequest, responder, _| {
                    responder.respond(RequestPermissionResponse::new(
                        RequestPermissionOutcome::Cancelled,
                    ))
                },
                sdk::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: sdk::UntypedMessage,
                            responder: agent_client_protocol::v1::Responder<serde_json::Value>,
                            _| {
                    responder.respond_with_error(
                        sdk::Error::method_not_found().data(request.method().to_owned()),
                    )
                },
                sdk::on_receive_request!(),
            )
            .connect_with(Lines::new(outgoing, incoming), async move |connection| {
                let _ = ready.send(connection);
                let _ = stopped.await;
                Ok(())
            })
            .await;
        if let Err(error) = result {
            eprintln!("Raiden's connection to {tool} ended: {error}");
        }
    });
    (connection, stop)
}

pub fn instructions_file(state: &Path, instructions: &str) -> PathBuf {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(instructions.as_bytes());
    let hex: String = digest[..6].iter().map(|b| format!("{b:02x}")).collect();
    state.join(format!("instructions-{hex}.md"))
}

#[cfg(test)]
pub(super) mod tests {
    use agent_client_protocol::v1::schema::{
        ContentChunk, ToolCall, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields,
    };

    use super::*;

    fn text(words: &str) -> SessionUpdate {
        SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
            words,
        ))))
    }

    fn heard(updates: &[SessionUpdate]) -> String {
        let mut answer = Answer::default();
        let mut written = String::new();
        for update in updates {
            answer.hear(update, &mut |piece: Heard<'_>| {
                if let Heard::Text(text) = piece {
                    written.push_str(text);
                }
            });
        }
        written
    }

    const FAKE_AGENT: &str = r#"#!/usr/bin/env python3
import json, os, sys
opened = 0
with open(os.environ["FAKE_LOG"], "a", buffering=1) as log:
    for line in sys.stdin:
        message = json.loads(line)
        log.write(json.dumps(message) + "\n")
        if "id" not in message or "method" not in message:
            continue
        if message["method"] == "initialize":
            result = {"protocolVersion": 1, "agentCapabilities": {}, "authMethods": []}
        elif message["method"] == "session/new":
            opened += 1
            result = {"sessionId": f"s{opened}"}
        else:
            result = {}
        print(json.dumps({"jsonrpc": "2.0", "id": message["id"], "result": result}), flush=True)
"#;

    pub(crate) struct Fake {
        pub(crate) adapter: Arc<Adapter>,
        log: PathBuf,
        folder: PathBuf,
    }

    impl Fake {
        pub(crate) async fn start(name: &str, provider: Provider, effort: Effort) -> Self {
            Self::launched(name, provider, effort, Web::Searchable).await
        }

        pub(crate) async fn launched(
            name: &str,
            provider: Provider,
            effort: Effort,
            web: Web,
        ) -> Self {
            use std::os::unix::fs::PermissionsExt as _;
            let folder = std::env::temp_dir()
                .join(format!("raiden-fake-agent-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&folder);
            std::fs::create_dir_all(&folder).expect("a folder");
            let program = folder.join("agent");
            std::fs::write(&program, FAKE_AGENT).expect("written");
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
                .expect("runnable");
            let log = folder.join("sent.log");
            let launch = Launch {
                provider,
                effort,
                web,
                program,
                env: vec![
                    ("PATH".into(), std::env::var("PATH").unwrap_or_default()),
                    ("FAKE_LOG".into(), log.display().to_string()),
                ],
                workdir: folder.clone(),
                instructions: "x".into(),
            };
            let adapter = Arc::new(Adapter::start(launch).await.expect("the fake agent"));
            Self {
                adapter,
                log,
                folder,
            }
        }

        fn sent(&self, method: &str) -> Vec<serde_json::Value> {
            std::fs::read_to_string(&self.log)
                .unwrap_or_default()
                .lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .filter(|message| message["method"] == method)
                .collect()
        }

        fn efforts_opened(&self) -> Vec<serde_json::Value> {
            self.sent("session/new")
                .iter()
                .map(|message| {
                    message["params"]["_meta"]["claudeCode"]["options"]["effort"].clone()
                })
                .collect()
        }

        async fn closed(&self, session: &Session) -> bool {
            for _ in 0..100 {
                if self
                    .sent("session/close")
                    .iter()
                    .any(|message| message["params"]["sessionId"] == session.id.to_string())
                {
                    return true;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            false
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.folder);
        }
    }

    #[tokio::test]
    async fn a_spare_session_is_reused_only_at_the_effort_it_was_opened_with() {
        let fake = Fake::start("spare", Provider::Claude, Effort::Low).await;
        let spare = fake.adapter.prepare(Effort::Low).await.expect("a spare");
        let again = fake
            .adapter
            .prepare(Effort::Low)
            .await
            .expect("the same spare");
        assert_eq!(again.id, spare.id);
        let high = fake
            .adapter
            .take(Web::Searchable, Effort::High)
            .await
            .expect("a session");
        assert_ne!(high.id, spare.id);
        assert_eq!(high.effort, Effort::High);
        assert!(fake.closed(&spare).await, "the low spare stays open");
        let medium = fake.adapter.prepare(Effort::Medium).await.expect("a spare");
        let replaced = fake.adapter.prepare(Effort::High).await.expect("a spare");
        assert_ne!(replaced.id, medium.id);
        assert!(fake.closed(&medium).await, "the medium spare stays open");
        let taken = fake
            .adapter
            .take(Web::Searchable, Effort::High)
            .await
            .expect("the spare");
        assert_eq!(taken.id, replaced.id);
        assert_eq!(
            fake.efforts_opened(),
            ["low", "high", "medium", "high"].map(serde_json::Value::from)
        );
    }

    #[tokio::test]
    async fn fix_grammar_sessions_open_at_the_effort_asked_for_and_skip_the_spare() {
        let fake = Fake::start("grammar", Provider::Claude, Effort::High).await;
        let spare = fake.adapter.prepare(Effort::High).await.expect("a spare");
        let grammar = fake
            .adapter
            .take(Web::Off, Effort::Low)
            .await
            .expect("a session");
        assert_ne!(grammar.id, spare.id);
        assert_eq!(grammar.effort, Effort::Low);
        let again = fake.adapter.prepare(Effort::High).await.expect("the spare");
        assert_eq!(again.id, spare.id);
        assert_eq!(
            fake.efforts_opened(),
            ["high", "low"].map(serde_json::Value::from)
        );
    }

    #[tokio::test]
    async fn codex_sessions_have_the_effort_its_process_started_with() {
        let fake = Fake::start("codex", Provider::Chatgpt, Effort::High).await;
        assert!(fake.adapter.serves(Effort::High));
        assert!(!fake.adapter.serves(Effort::Low));
        let session = fake
            .adapter
            .take(Web::Searchable, Effort::Low)
            .await
            .expect("a session");
        assert_eq!(session.effort, Effort::High);
        assert!(fake.sent("session/new")[0]["params"].get("_meta").is_none());
        let spare = fake.adapter.prepare(Effort::High).await.expect("a spare");
        let again = fake
            .adapter
            .prepare(Effort::Low)
            .await
            .expect("the same spare");
        assert_eq!(again.id, spare.id);
        let claude = Fake::start("claude-any", Provider::Claude, Effort::Low).await;
        assert!(
            Effort::ALL
                .into_iter()
                .all(|effort| claude.adapter.serves(effort))
        );
    }

    #[tokio::test]
    async fn codex_sessions_search_the_web_only_when_its_process_started_able_to() {
        let fake = Fake::launched("codex-off", Provider::Chatgpt, Effort::Low, Web::Off).await;
        assert!(fake.adapter.searches(Web::Off));
        assert!(!fake.adapter.searches(Web::Searchable));
        let spare = fake.adapter.prepare(Effort::Low).await.expect("a spare");
        assert_eq!(spare.web, Web::Off);
        let taken = fake
            .adapter
            .take(Web::Off, Effort::Low)
            .await
            .expect("the spare");
        assert_eq!(taken.id, spare.id);
        let asked_to_search = fake
            .adapter
            .take(Web::Searchable, Effort::Low)
            .await
            .expect("a session");
        assert_eq!(asked_to_search.web, Web::Off);
        let claude = Fake::start("claude-web", Provider::Claude, Effort::Low).await;
        assert!(claude.adapter.searches(Web::Off));
        assert!(claude.adapter.searches(Web::Searchable));
    }

    #[tokio::test]
    async fn a_codex_able_to_search_never_hands_out_a_session_without_the_web() {
        let fake = Fake::start("codex-on", Provider::Chatgpt, Effort::Low).await;
        fake.adapter.prepare(Effort::Low).await.expect("a spare");
        assert!(fake.adapter.take(Web::Off, Effort::Low).await.is_err());
        assert!(fake.adapter.open(Web::Off, Effort::Low).await.is_err());
        assert_eq!(fake.sent("session/new").len(), 1);
        let searching = fake
            .adapter
            .take(Web::Searchable, Effort::Low)
            .await
            .expect("the spare");
        assert_eq!(searching.web, Web::Searchable);
    }

    #[test]
    fn a_search_is_told_once_per_query_and_never_written() {
        let started = SessionUpdate::ToolCall(ToolCall::new("1", "Web search"));
        let query = |words: &str| {
            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                "1",
                ToolCallUpdateFields::new().raw_input(serde_json::json!({ "query": words })),
            ))
        };
        let mut told = Vec::new();
        let mut answer = Answer::default();
        for update in [
            started,
            query("usd to mxn"),
            query("usd to mxn"),
            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                "1",
                ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
            )),
            text("About 18."),
        ] {
            answer.hear(&update, &mut |piece: Heard<'_>| {
                told.push(format!("{piece:?}"))
            });
        }
        assert_eq!(
            told,
            [
                "Searching(\"\")",
                "Searching(\"usd to mxn\")",
                "Text(\"About 18.\")"
            ]
        );
    }

    #[test]
    fn words_after_a_search_start_a_new_paragraph() {
        let search = SessionUpdate::ToolCall(ToolCall::new("1", "Web search"));
        assert_eq!(
            heard(&[
                text("Looking."),
                search.clone(),
                text("It is 1.100"),
                text(".")
            ]),
            "Looking.\n\nIt is 1.100."
        );
        assert_eq!(
            heard(&[search.clone(), text("It is 1.100.")]),
            "It is 1.100."
        );
        assert_eq!(heard(&[text("It is "), text("1.100.")]), "It is 1.100.");
        assert_eq!(
            heard(&[text("Looking.\n"), search.clone(), text("It is 1.100.")]),
            "Looking.\n\nIt is 1.100."
        );
        assert_eq!(
            heard(&[text("Looking."), search.clone(), text("\nIt is 1.100.")]),
            "Looking.\n\nIt is 1.100."
        );
        assert_eq!(
            heard(&[
                text("Looking."),
                search.clone(),
                text(" "),
                search.clone(),
                text("It is 1.100.")
            ]),
            "Looking. \n\nIt is 1.100."
        );
        assert_eq!(
            heard(&[text("Looking."), search.clone(), text("\n\nIt is 1.100.")]),
            "Looking.\n\nIt is 1.100."
        );
        assert_eq!(
            heard(&[
                text("Looking."),
                search.clone(),
                text(" "),
                text("It is 1.100.")
            ]),
            "Looking. \n\nIt is 1.100."
        );
        assert_eq!(
            heard(&[text(" "), search, text("It is 1.100.")]),
            " It is 1.100."
        );
    }
}
