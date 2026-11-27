use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::{Duration, Instant};

use agent_client_protocol::v1::schema::StopReason;
use raiden_core::ai::{
    Account, Answer, AnswerKind, Effort, Exchange, Prepared, Provider, Tag, Web, environment,
    outgoing,
};
use tokio::sync::oneshot;

use super::adapter::{self, Adapter, Ending, Heard, Launch, Session, Turn};
use super::install;
use super::process;

const STATUS_LIMIT: Duration = Duration::from_secs(15);
const SIGN_IN_LIMIT: Duration = Duration::from_secs(600);
const SHELL_LIMIT: Duration = Duration::from_secs(5);
const RETRY_AFTER: Duration = Duration::from_secs(300);

pub type Sink = Arc<dyn Fn(Answer) + Send + Sync>;

type Hidden = BTreeMap<String, usize>;

struct Thread {
    adapter: Weak<Adapter>,
    session: Option<Session>,
    earlier: Vec<Exchange>,
    web: Web,
}

impl Thread {
    fn new(earlier: Vec<Exchange>, web: Web) -> Self {
        Self {
            adapter: Weak::new(),
            session: None,
            earlier,
            web,
        }
    }

    fn web_for(&mut self, context: &[Tag]) -> Web {
        if raiden_core::ai::web_for(context) == Web::Off {
            self.web = Web::Off;
        }
        self.web
    }

    fn close(&mut self) {
        let session = self.session.take();
        if let (Some(adapter), Some(session)) = (self.adapter.upgrade(), session) {
            adapter.close(&session);
        }
    }
}

struct Opened {
    adapter: Arc<Adapter>,
    session: Session,
    fresh: bool,
}

type Spoken = (Session, Turn, Hidden);

pub struct Engine {
    state: PathBuf,
    instructions: String,
    effort: Mutex<Effort>,
    path: tokio::sync::OnceCell<String>,
    adapters: HashMap<Provider, tokio::sync::Mutex<Option<Arc<Adapter>>>>,
    failed: Mutex<HashMap<Provider, Instant>>,
    answers: Mutex<HashMap<String, Option<oneshot::Sender<()>>>>,
    threads: Mutex<HashMap<String, Arc<tokio::sync::Mutex<Thread>>>>,
    offline: Mutex<HashSet<String>>,
    next: AtomicU64,
}

impl Engine {
    pub fn new(state: PathBuf, instructions: &str) -> Self {
        Self {
            state: state.join("ai"),
            instructions: instructions.to_owned(),
            effort: Mutex::new(Effort::default()),
            path: tokio::sync::OnceCell::new(),
            adapters: Provider::ALL
                .into_iter()
                .map(|provider| (provider, tokio::sync::Mutex::new(None)))
                .collect(),
            failed: Mutex::new(HashMap::new()),
            answers: Mutex::new(HashMap::new()),
            threads: Mutex::new(HashMap::new()),
            offline: Mutex::new(HashSet::new()),
            next: AtomicU64::new(1),
        }
    }

    pub fn effort(&self) -> Effort {
        *self.effort.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn set_effort(&self, effort: Effort) -> bool {
        let mut held = self.effort.lock().unwrap_or_else(PoisonError::into_inner);
        std::mem::replace(&mut *held, effort) != effort
    }

    pub async fn drop_stale(&self) {
        let effort = self.effort();
        for slot in self.adapters.values() {
            let mut slot = slot.lock().await;
            if slot.as_ref().is_some_and(|adapter| !adapter.serves(effort)) {
                slot.take();
            }
        }
    }

    fn prepare_later(self: &Arc<Self>, adapter: Arc<Adapter>) {
        let engine = Arc::clone(self);
        tokio::spawn(async move {
            let _ = adapter.prepare(engine.effort()).await;
        });
    }

    pub async fn path(&self) -> &str {
        self.path
            .get_or_init(|| async {
                match login_path().await {
                    Some(path) => path,
                    None => std::env::var("PATH")
                        .map(|path| format!("{path}:{}", environment::SYSTEM_PATH))
                        .unwrap_or_else(|_| environment::SYSTEM_PATH.to_owned()),
                }
            })
            .await
    }

    async fn env(&self) -> Vec<(String, String)> {
        let path = self.path().await.to_owned();
        environment::allowed(std::env::vars(), Some(&path))
    }

    async fn program(&self, provider: Provider) -> Option<PathBuf> {
        environment::which(provider.program(), self.path().await)
    }

    fn workdir(&self, provider: Provider) -> PathBuf {
        self.state.join("empty").join(provider.id())
    }

    pub async fn account(&self, provider: Provider) -> Account {
        let signed_in = match self.program(provider).await {
            Some(program) => {
                let env = self.env().await;
                let workdir = self.workdir(provider);
                let _ = tokio::fs::create_dir_all(&workdir).await;
                let mut command = process::command(&program, &env, &workdir);
                command.args(provider.status_args());
                process::run(command, STATUS_LIMIT)
                    .await
                    .is_ok_and(|ran| provider.signed_in(ran.succeeded, &ran.output))
            }
            None => false,
        };
        Account::new(provider, signed_in)
    }

    pub async fn accounts(&self) -> Vec<Account> {
        let (claude, chatgpt) = tokio::join!(
            self.account(Provider::Claude),
            self.account(Provider::Chatgpt)
        );
        vec![claude, chatgpt]
    }

    pub async fn sign_in(&self, provider: Provider) -> Result<bool, String> {
        let program = self
            .program(provider)
            .await
            .ok_or_else(|| format!("{} is not installed", provider.through()))?;
        let env = self.env().await;
        let workdir = self.workdir(provider);
        let _ = tokio::fs::create_dir_all(&workdir).await;
        let mut command = process::command(&program, &env, &workdir);
        command.args(provider.sign_in_args());
        process::run(command, SIGN_IN_LIMIT)
            .await
            .map_err(|error| format!("{}'s sign-in failed: {error}", provider.through()))?;
        self.drop_adapter(provider).await;
        Ok(self.account(provider).await.signed_in)
    }

    pub async fn drop_adapter(&self, provider: Provider) {
        if let Some(slot) = self.adapters.get(&provider) {
            slot.lock().await.take();
        }
    }

    async fn launch(&self, provider: Provider, effort: Effort, web: Web) -> Result<Launch, String> {
        let executable = self
            .program(provider)
            .await
            .ok_or_else(|| format!("{} is not installed", provider.through()))?;
        let path = self.path().await.to_owned();
        let mut env = self.env().await;
        let root = self.state.join("adapters");
        let package = install::package(provider);
        let program = install::ensure(package, &root, &env, &path).await?;
        let workdir = self.workdir(provider);
        tokio::fs::create_dir_all(&workdir)
            .await
            .map_err(|error| format!("Raiden could not make its empty folder: {error}"))?;
        env.push((
            provider.executable_variable().to_owned(),
            executable.display().to_string(),
        ));
        if provider == Provider::Chatgpt {
            let instructions = adapter::instructions_file(&self.state, &self.instructions);
            tokio::fs::write(&instructions, &self.instructions)
                .await
                .map_err(|error| format!("Raiden could not write its instructions: {error}"))?;
            let config =
                raiden_core::ai::codex_config(&instructions.display().to_string(), effort, web);
            env.extend(
                [
                    ("CODEX_CONFIG", config.to_string()),
                    ("INITIAL_AGENT_MODE", "read-only".to_owned()),
                    ("NO_BROWSER", "1".to_owned()),
                    ("CODEX_ACP_PROMPT_SUGGESTIONS", "false".to_owned()),
                ]
                .map(|(name, value)| (name.to_owned(), value)),
            );
        }
        Ok(Launch {
            provider,
            effort,
            web,
            program,
            env,
            workdir,
            instructions: self.instructions.clone(),
        })
    }

    pub async fn adapter(
        &self,
        provider: Provider,
        web: Option<Web>,
    ) -> Result<Arc<Adapter>, String> {
        let slot = self
            .adapters
            .get(&provider)
            .ok_or_else(|| format!("Raiden has no adapter for {}", provider.through()))?;
        let mut slot = slot.lock().await;
        let effort = self.effort();
        if let Some(adapter) = fitting(&mut slot, effort, web) {
            return Ok(adapter);
        }
        let started = match self
            .launch(provider, effort, web.unwrap_or(Web::Searchable))
            .await
        {
            Ok(launch) => Adapter::start(launch).await,
            Err(error) => Err(error),
        };
        if let Ok(mut failed) = self.failed.lock() {
            match started {
                Ok(_) => failed.remove(&provider),
                Err(_) => failed.insert(provider, Instant::now()),
            };
        }
        let adapter = Arc::new(started?);
        *slot = Some(Arc::clone(&adapter));
        Ok(adapter)
    }

    pub async fn warm(&self, provider: Provider) -> Result<Session, String> {
        let failed_recently = self.failed.lock().ok().and_then(|failed| {
            failed
                .get(&provider)
                .filter(|at| at.elapsed() < RETRY_AFTER)
                .copied()
        });
        if failed_recently.is_some() {
            return Err(format!(
                "{} failed to start recently, so Raiden waits before trying again",
                provider.through()
            ));
        }
        self.adapter(provider, None)
            .await?
            .prepare(self.effort())
            .await
    }

    pub async fn rest(&self) {
        self.threads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        futures::future::join_all(
            self.adapters
                .values()
                .map(|slot| async move { slot.lock().await.take() }),
        )
        .await;
    }

    async fn forget(&self, provider: Provider, stuck: &Arc<Adapter>) {
        if let Some(slot) = self.adapters.get(&provider) {
            let mut slot = slot.lock().await;
            if slot.as_ref().is_some_and(|held| Arc::ptr_eq(held, stuck)) {
                slot.take();
            }
        }
    }

    pub fn stop(&self, id: &str) -> bool {
        let stop = self
            .answers
            .lock()
            .ok()
            .and_then(|mut answers| answers.get_mut(id).and_then(Option::take));
        stop.is_some_and(|stop| stop.send(()).is_ok())
    }

    pub fn keep_off_the_web(&self, conversation: &str) {
        self.offline
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(conversation.to_owned());
    }

    pub fn web(&self, conversation: &str) -> Web {
        let offline = self.offline.lock().unwrap_or_else(PoisonError::into_inner);
        if offline.contains(conversation) {
            Web::Off
        } else {
            Web::Searchable
        }
    }

    fn thread(&self, conversation: &str, saved: Vec<Exchange>) -> Arc<tokio::sync::Mutex<Thread>> {
        let web = self.web(conversation);
        let mut threads = self.threads.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(
            threads
                .entry(conversation.to_owned())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(Thread::new(saved, web)))),
        )
    }

    pub fn knows(&self, conversation: &str) -> bool {
        self.threads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(conversation)
    }

    pub fn end(&self, conversation: &str) {
        let ended = self
            .threads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(conversation);
        if let Some(thread) = ended {
            tokio::spawn(async move {
                thread.lock().await.close();
            });
        }
    }

    pub fn ask(
        self: &Arc<Self>,
        provider: Provider,
        conversation: String,
        saved: Vec<Exchange>,
        asked: (String, Vec<Tag>),
        answer: Option<String>,
        sink: Sink,
    ) -> Result<String, String> {
        let id = answer
            .unwrap_or_else(|| format!("raiden-{}", self.next.fetch_add(1, Ordering::Relaxed)));
        let (stop, stopped) = oneshot::channel();
        {
            let mut answers = self
                .answers
                .lock()
                .map_err(|_| "Raiden lost track of its answers".to_owned())?;
            if answers.contains_key(&id) {
                return Err(format!("An answer with the id {id:?} is already running"));
            }
            answers.insert(id.clone(), Some(stop));
        }
        if raiden_core::ai::web_for(&asked.1) == Web::Off {
            self.keep_off_the_web(&conversation);
        }
        let thread = self.thread(&conversation, saved);
        let engine = Arc::clone(self);
        let answer_id = id.clone();
        tokio::spawn(async move {
            engine
                .answer(provider, &thread, &answer_id, asked, stopped, sink)
                .await;
            if let Ok(mut answers) = engine.answers.lock() {
                answers.remove(&answer_id);
            }
        });
        Ok(id)
    }

    async fn ready(
        self: &Arc<Self>,
        provider: Provider,
        web: Web,
        stopped: &mut oneshot::Receiver<()>,
    ) -> Option<Result<(Arc<Adapter>, Session), String>> {
        let engine = Arc::clone(self);
        let mut starting = tokio::spawn(async move {
            let adapter = engine.adapter(provider, Some(web)).await?;
            let session = adapter.take(web, engine.effort()).await?;
            Ok::<_, String>((adapter, session))
        });
        tokio::select! {
            started = &mut starting => Some(started.unwrap_or_else(|error| Err(error.to_string()))),
            _ = stopped => {
                tokio::spawn(async move {
                    if let Ok(Ok((adapter, session))) = starting.await {
                        adapter.close(&session);
                    }
                });
                None
            }
        }
    }

    async fn held(
        &self,
        provider: Provider,
        web: Web,
        thread: &Thread,
    ) -> Option<(Arc<Adapter>, Session)> {
        let adapter = thread.adapter.upgrade()?;
        let session = thread.session.clone()?;
        if adapter.provider != provider || session.web != web || !adapter.running() {
            return None;
        }
        let slot = self.adapters.get(&provider)?.lock().await;
        slot.as_ref()
            .filter(|current| Arc::ptr_eq(current, &adapter))?;
        Some((adapter, session))
    }

    async fn open_turn(
        self: &Arc<Self>,
        provider: Provider,
        web: Web,
        thread: &mut Thread,
        stopped: &mut oneshot::Receiver<()>,
    ) -> Option<Result<Opened, String>> {
        let held = tokio::select! {
            held = self.held(provider, web, thread) => held,
            _ = &mut *stopped => return None,
        };
        if let Some((adapter, session)) = held {
            return Some(Ok(Opened {
                adapter,
                session,
                fresh: false,
            }));
        }
        thread.close();
        let ready = self.ready(provider, web, stopped).await?;
        Some(ready.map(|(adapter, session)| {
            thread.adapter = Arc::downgrade(&adapter);
            thread.session = Some(session.clone());
            self.prepare_later(Arc::clone(&adapter));
            Opened {
                adapter,
                session,
                fresh: true,
            }
        }))
    }

    async fn converse(
        self: &Arc<Self>,
        provider: Provider,
        thread: &mut Thread,
        question: &str,
        context: &[Tag],
        stopped: &mut oneshot::Receiver<()>,
        mut on_heard: impl FnMut(Heard<'_>, &str, &Hidden),
    ) -> Option<Result<Spoken, String>> {
        let mut retried = false;
        let web = thread.web_for(context);
        loop {
            let opened = match self.open_turn(provider, web, thread, stopped).await? {
                Ok(opened) => opened,
                Err(error) => return Some(Err(error)),
            };
            let out = outgoing(&thread.earlier, opened.fresh, question, context);
            let mut said = String::new();
            let model = opened.session.model.clone();
            let turn = opened
                .adapter
                .prompt(&opened.session, &out.sent.text, stopped, |heard| {
                    if let Heard::Text(text) = heard {
                        said.push_str(text);
                    }
                    on_heard(heard, &model, &out.sent.hidden);
                })
                .await;
            match &turn {
                Ok(turn) => {
                    if turn.sent && finished(turn.ending) {
                        thread.earlier.push(Exchange {
                            asked: out.asked.text,
                            answer: said,
                        });
                    }
                    if turn.unresponsive {
                        self.forget(provider, &opened.adapter).await;
                        thread.session = None;
                    } else if !finished(turn.ending) {
                        thread.close();
                    }
                }
                Err(_) => {
                    thread.close();
                    if !opened.fresh && !retried && said.is_empty() {
                        retried = true;
                        continue;
                    }
                }
            }
            return Some(turn.map(|turn| (opened.session, turn, out.sent.hidden)));
        }
    }

    async fn answer(
        self: &Arc<Self>,
        provider: Provider,
        thread: &tokio::sync::Mutex<Thread>,
        id: &str,
        asked: (String, Vec<Tag>),
        mut stopped: oneshot::Receiver<()>,
        sink: Sink,
    ) {
        let (question, context) = asked;
        let none = Hidden::new();
        let say = |kind: AnswerKind, text: String, model: &str, hidden: &Hidden| {
            sink(Answer {
                id: id.to_owned(),
                kind,
                text,
                model: model.to_owned(),
                hidden: hidden.clone(),
            });
        };
        let mut thread = tokio::select! {
            thread = thread.lock() => thread,
            _ = &mut stopped => return say(AnswerKind::Done, String::new(), "", &none),
        };
        let mut said = String::new();
        let spoken = self
            .converse(
                provider,
                &mut thread,
                &question,
                &context,
                &mut stopped,
                |heard, model, hidden| match heard {
                    Heard::Text(text) => {
                        said.push_str(text);
                        say(AnswerKind::Delta, text.to_owned(), model, hidden);
                    }
                    Heard::Searching(query) => {
                        say(AnswerKind::Searching, query.to_owned(), model, hidden);
                    }
                },
            )
            .await;
        let (session, turn, hidden) = match spoken {
            Some(Ok(spoken)) => spoken,
            Some(Err(error)) => return say(AnswerKind::Error, error, "", &none),
            None => return say(AnswerKind::Done, String::new(), "", &none),
        };
        let model = session.model.as_str();
        match turn.ending {
            Ending::Answered(StopReason::EndTurn | StopReason::Cancelled) | Ending::Stopped => {
                say(AnswerKind::Done, said, model, &hidden);
            }
            Ending::Answered(stop) => say(
                AnswerKind::Error,
                format!("{} stopped early ({stop:?})", provider.through()),
                model,
                &hidden,
            ),
            Ending::TimedOut => say(
                AnswerKind::Error,
                format!(
                    "{} was stopped after {} minutes without finishing",
                    provider.through(),
                    adapter::ANSWER_LIMIT.as_secs() / 60
                ),
                model,
                &hidden,
            ),
        }
    }

    pub async fn ask_within(
        self: &Arc<Self>,
        provider: Provider,
        prepared: Prepared,
        (web, effort): (Web, Effort),
        limit: Duration,
        mut on_text: impl FnMut(&str),
    ) -> Result<(Session, Turn), String> {
        let adapter = self.adapter(provider, Some(web)).await?;
        let session = adapter.take(web, effort).await?;
        self.prepare_later(Arc::clone(&adapter));
        let (stop, mut stopped) = oneshot::channel();
        let timer = tokio::spawn(async move {
            tokio::time::sleep(limit).await;
            let _ = stop.send(());
        });
        let turn = adapter
            .prompt(&session, &prepared.text, &mut stopped, |heard| {
                if let Heard::Text(text) = heard {
                    on_text(text);
                }
            })
            .await;
        timer.abort();
        adapter.close(&session);
        if turn.as_ref().is_ok_and(|turn| turn.unresponsive) {
            self.forget(provider, &adapter).await;
        }
        Ok((session, turn?))
    }
}

fn fitting(
    slot: &mut Option<Arc<Adapter>>,
    effort: Effort,
    web: Option<Web>,
) -> Option<Arc<Adapter>> {
    let fits = slot.as_ref().is_some_and(|adapter| {
        adapter.running() && adapter.serves(effort) && web.is_none_or(|web| adapter.searches(web))
    });
    if fits {
        slot.clone()
    } else {
        slot.take();
        None
    }
}

fn finished(ending: Ending) -> bool {
    matches!(
        ending,
        Ending::Answered(StopReason::EndTurn | StopReason::Cancelled) | Ending::Stopped
    )
}

async fn login_path() -> Option<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_owned());
    let mut command = tokio::process::Command::new(&shell);
    command
        .args(["-ilc", &environment::login_path_script()])
        .env_clear()
        .envs(std::env::vars().filter(|(name, _)| {
            ["HOME", "USER", "LOGNAME", "TMPDIR", "LANG"].contains(&name.as_str())
        }))
        .env("SHELL", &shell)
        .env("PATH", environment::SYSTEM_PATH)
        .current_dir("/");
    let ran = process::run(command, SHELL_LIMIT).await.ok()?;
    environment::login_path(&ran.output)
}

#[cfg(test)]
mod tests {
    use super::super::adapter::tests::Fake;
    use super::*;

    async fn held(engine: &Engine, provider: Provider) -> bool {
        engine.adapters[&provider].lock().await.is_some()
    }

    #[tokio::test]
    async fn a_new_effort_stops_codex_and_keeps_claude_running() {
        let folder = std::env::temp_dir().join(format!("raiden-engine-{}", std::process::id()));
        let engine = Engine::new(folder.clone(), "x");
        assert_eq!(engine.effort(), Effort::Low);
        let codex = Fake::start("engine-codex", Provider::Chatgpt, Effort::Low).await;
        let claude = Fake::start("engine-claude", Provider::Claude, Effort::Low).await;
        *engine.adapters[&Provider::Chatgpt].lock().await = Some(Arc::clone(&codex.adapter));
        *engine.adapters[&Provider::Claude].lock().await = Some(Arc::clone(&claude.adapter));
        assert!(!engine.set_effort(Effort::Low));
        engine.drop_stale().await;
        assert!(held(&engine, Provider::Chatgpt).await);
        assert!(engine.set_effort(Effort::High));
        assert_eq!(engine.effort(), Effort::High);
        engine.drop_stale().await;
        assert!(!held(&engine, Provider::Chatgpt).await);
        assert!(held(&engine, Provider::Claude).await);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_conversation_that_once_held_pull_requests_never_searches_the_web_again() {
        let pulls = Tag {
            kind: raiden_core::ai::PULLS_TAG.to_owned(),
            label: "Open pull requests".to_owned(),
            text: "fernhill/beacon #1".to_owned(),
        };
        let mut plain = Thread::new(Vec::new(), Web::Searchable);
        assert_eq!(plain.web_for(&[]), Web::Searchable);
        let mut thread = Thread::new(Vec::new(), Web::Searchable);
        assert_eq!(thread.web_for(std::slice::from_ref(&pulls)), Web::Off);
        assert_eq!(thread.web_for(&[]), Web::Off);
        assert_eq!(thread.web_for(&[]), Web::Off);
    }

    #[tokio::test]
    async fn a_conversation_reopened_off_the_web_starts_its_thread_off_the_web() {
        let folder = std::env::temp_dir().join(format!("raiden-offline-{}", std::process::id()));
        let engine = Engine::new(folder.clone(), "x");
        assert_eq!(engine.web("pulls-c1"), Web::Searchable);
        engine.keep_off_the_web("pulls-c1");
        assert_eq!(engine.web("pulls-c1"), Web::Off);
        assert_eq!(engine.web("ask-c1"), Web::Searchable);
        let reopened = engine.thread("pulls-c1", Vec::new());
        assert_eq!(reopened.lock().await.web_for(&[]), Web::Off);
        let plain = engine.thread("ask-c1", Vec::new());
        assert_eq!(plain.lock().await.web_for(&[]), Web::Searchable);
        engine.end("pulls-c1");
        assert_eq!(engine.web("pulls-c1"), Web::Off);
    }

    #[tokio::test]
    async fn a_chatgpt_question_without_the_web_stops_a_codex_that_can_search() {
        let codex = Fake::start("engine-web-codex", Provider::Chatgpt, Effort::Low).await;
        let claude = Fake::start("engine-web-claude", Provider::Claude, Effort::Low).await;
        let mut codex_slot = Some(Arc::clone(&codex.adapter));
        let mut claude_slot = Some(Arc::clone(&claude.adapter));
        assert!(fitting(&mut codex_slot, Effort::Low, None).is_some());
        assert!(fitting(&mut codex_slot, Effort::Low, Some(Web::Searchable)).is_some());
        assert!(fitting(&mut claude_slot, Effort::Low, Some(Web::Off)).is_some());
        assert!(claude_slot.is_some());
        assert!(fitting(&mut codex_slot, Effort::Low, Some(Web::Off)).is_none());
        assert!(codex_slot.is_none());
        let closed =
            Fake::launched("engine-web-off", Provider::Chatgpt, Effort::Low, Web::Off).await;
        let mut closed_slot = Some(Arc::clone(&closed.adapter));
        assert!(fitting(&mut closed_slot, Effort::Low, Some(Web::Off)).is_some());
        assert!(fitting(&mut closed_slot, Effort::Low, None).is_some());
        assert!(fitting(&mut closed_slot, Effort::Low, Some(Web::Searchable)).is_none());
    }
}
