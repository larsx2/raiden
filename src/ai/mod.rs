mod adapter;
mod engine;
mod install;
mod process;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use agent_client_protocol::v1::schema::StopReason;
use raiden_core::ai::{Account, Effort, Exchange, Prepared, Provider, Tag, Web};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::answers::Answers;

pub use engine::Engine;

const REST_AFTER: Duration = Duration::from_secs(20);
const REWRITE_LIMIT: Duration = Duration::from_secs(60);

pub struct Ai {
    engine: Arc<Engine>,
    shown: AtomicU64,
}

impl Ai {
    pub fn new(development: bool) -> Self {
        Self {
            engine: Arc::new(Engine::new(
                crate::folders(development).1,
                raiden_core::ai::INSTRUCTIONS,
            )),
            shown: AtomicU64::new(0),
        }
    }
}

impl Ai {
    pub fn web(&self, conversation: &str) -> Web {
        self.engine.web(conversation)
    }
}

fn provider(id: &str) -> Result<Provider, String> {
    Provider::from_id(id).ok_or_else(|| format!("Raiden has no AI called {id:?}"))
}

pub async fn login_path(app: &AppHandle) -> String {
    let engine = Arc::clone(&app.state::<Ai>().engine);
    engine.path().await.to_owned()
}

pub fn warm(app: &AppHandle) {
    let ai = app.state::<Ai>();
    ai.shown.fetch_add(1, Ordering::SeqCst);
    warm_up(app, crate::settings::ai(app));
}

pub fn warm_up(app: &AppHandle, provider: Provider) {
    let engine = Arc::clone(&app.state::<Ai>().engine);
    tauri::async_runtime::spawn(async move {
        if let Err(error) = engine.warm(provider).await {
            eprintln!("Raiden could not get {} ready: {error}", provider.through());
        }
    });
}

pub fn set_effort(app: &AppHandle, effort: Effort) -> bool {
    let engine = Arc::clone(&app.state::<Ai>().engine);
    if !engine.set_effort(effort) {
        return false;
    }
    tauri::async_runtime::spawn(async move { engine.drop_stale().await });
    true
}

pub fn rest_later(app: &AppHandle) {
    let ai = app.state::<Ai>();
    let shown = ai.shown.load(Ordering::SeqCst);
    let engine = Arc::clone(&ai.engine);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(REST_AFTER).await;
        if app.state::<Ai>().shown.load(Ordering::SeqCst) == shown {
            engine.rest().await;
        }
    });
}

#[tauri::command]
pub async fn ai_accounts(ai: State<'_, Ai>) -> Result<Vec<Account>, ()> {
    Ok(ai.engine.accounts().await)
}

#[tauri::command]
pub async fn ai_sign_in(ai: State<'_, Ai>, id: String) -> Result<bool, String> {
    ai.engine.sign_in(provider(&id)?).await
}

#[tauri::command]
pub async fn ask(
    app: AppHandle,
    ai: State<'_, Ai>,
    id: String,
    conversation: String,
    question: String,
    context: Vec<Tag>,
    answer: Option<String>,
) -> Result<String, String> {
    let provider = provider(&id)?;
    let kept = if ai.engine.knows(&conversation) {
        None
    } else {
        app.state::<Answers>().conversation(&conversation)
    };
    if kept.as_ref().is_some_and(|kept| kept.web == Web::Off) {
        ai.engine.keep_off_the_web(&conversation);
    }
    let saved: Vec<Exchange> = kept
        .map(|kept| {
            kept.turns
                .into_iter()
                .map(|turn| Exchange {
                    asked: turn.question,
                    answer: turn.answer,
                })
                .collect()
        })
        .unwrap_or_default();
    let sink: engine::Sink = Arc::new(move |answer| {
        if let Err(error) = app.emit_to(crate::PANEL, "answer", &answer) {
            eprintln!("Raiden could not send an answer to its window: {error}");
        }
    });
    ai.engine.ask(
        provider,
        conversation,
        saved,
        (question, context),
        answer,
        sink,
    )
}

#[tauri::command]
pub async fn ai_end(ai: State<'_, Ai>, conversation: String) -> Result<(), ()> {
    ai.engine.end(&conversation);
    Ok(())
}

#[tauri::command]
pub async fn ai_stop(ai: State<'_, Ai>, id: String) -> Result<bool, ()> {
    Ok(ai.engine.stop(&id))
}

pub async fn rewrite(app: &AppHandle, prepared: Prepared) -> Result<String, String> {
    let engine = Arc::clone(&app.state::<Ai>().engine);
    let provider = Provider::Claude;
    let mut said = String::new();
    let (_, turn) = engine
        .ask_within(
            provider,
            prepared,
            (Web::Off, Effort::Low),
            REWRITE_LIMIT,
            |text| said.push_str(text),
        )
        .await?;
    if turn.searched {
        return Err(format!(
            "{} searched the web instead of fixing the text, so Raiden left it as it was",
            provider.through()
        ));
    }
    match turn.ending {
        adapter::Ending::Answered(StopReason::EndTurn) if !said.trim().is_empty() => Ok(said),
        adapter::Ending::Answered(StopReason::EndTurn) => Err(format!(
            "{} gave no answer, so Raiden left the text as it was",
            provider.through()
        )),
        adapter::Ending::Stopped | adapter::Ending::TimedOut => Err(format!(
            "{} took over {} s, so Raiden left the text as it was",
            provider.through(),
            REWRITE_LIMIT.as_secs()
        )),
        adapter::Ending::Answered(_) => Err(format!(
            "{} did not finish, so Raiden left the text as it was",
            provider.through()
        )),
    }
}
