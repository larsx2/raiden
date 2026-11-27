use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use raiden_core::ai::Web;
use raiden_core::answers::{self, Conversation, Saved};
use tauri::State;

pub struct Answers {
    file: PathBuf,
    lock: Mutex<()>,
}

impl Answers {
    pub fn new(state: &Path) -> Self {
        Self {
            file: state.join(answers::FILE),
            lock: Mutex::new(()),
        }
    }

    fn read(&self) -> String {
        std::fs::read_to_string(&self.file).unwrap_or_default()
    }

    fn recent(&self) -> Vec<Conversation> {
        let _reading = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        answers::recent(&self.read())
    }

    pub fn conversation(&self, id: &str) -> Option<Conversation> {
        let _reading = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        answers::find(&self.read(), id)
    }

    fn keep(&self, mut saved: Saved) -> std::io::Result<Option<Conversation>> {
        let _writing = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        if answers::find(&self.read(), &saved.conversation).is_some_and(|kept| kept.web == Web::Off)
        {
            saved.web = Web::Off;
        }
        let text = append(&self.file, &saved)?;
        Ok(answers::find(&text, &saved.conversation))
    }
}

fn replace(file: &Path, text: &str) -> std::io::Result<()> {
    let spare = file.with_extension("new");
    std::fs::write(&spare, text)?;
    std::fs::rename(&spare, file)
}

fn append(file: &Path, saved: &Saved) -> std::io::Result<String> {
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)?
        .write_all(answers::line(saved).as_bytes())?;
    let text = std::fs::read_to_string(file)?;
    match answers::kept(&text) {
        Some(kept) => {
            replace(file, &kept)?;
            Ok(kept)
        }
        None => Ok(text),
    }
}

#[tauri::command]
pub async fn conversations(answers: State<'_, Answers>) -> Result<Vec<Conversation>, ()> {
    Ok(answers.recent())
}

#[tauri::command]
pub async fn save_turn(
    answers: State<'_, Answers>,
    ai: State<'_, crate::ai::Ai>,
    conversation: String,
    question: String,
    model: String,
    answer: String,
) -> Result<Conversation, String> {
    let web = ai.web(&conversation);
    let saved = answers::hidden(Saved {
        conversation,
        at: crate::picks::now(),
        question,
        model,
        answer,
        web,
    });
    if saved.conversation.trim().is_empty()
        || saved.question.trim().is_empty()
        || saved.answer.trim().is_empty()
    {
        return Err("Raiden keeps only answers to a question.".to_owned());
    }
    answers
        .keep(saved)
        .map_err(|error| format!("Raiden could not keep the answer: {error}"))?
        .ok_or_else(|| "Raiden could not read back the conversation it kept.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::Answers;
    use raiden_core::ai::Web;
    use raiden_core::answers::{KEPT, Saved};

    #[test]
    fn trims_by_replacing_the_file_whole() {
        let folder = std::env::temp_dir().join(format!("raiden-answers-{}", std::process::id()));
        let answers = Answers::new(&folder);
        for at in 0..(KEPT as u64 + 5) {
            let saved = Saved {
                conversation: format!("c{at}"),
                at,
                question: format!("q{at}"),
                model: "haiku".into(),
                answer: "a".into(),
                web: Web::Searchable,
            };
            answers.keep(saved).unwrap();
        }
        let recent = answers.recent();
        let left: Vec<String> = std::fs::read_dir(&folder)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(recent.len(), KEPT);
        assert_eq!(recent.first().map(|kept| kept.at), Some(KEPT as u64 + 4));
        assert_eq!(left, ["answers"]);
    }

    #[test]
    fn a_conversation_kept_off_the_web_stays_off_in_later_saves() {
        let folder = std::env::temp_dir().join(format!("raiden-no-web-{}", std::process::id()));
        let answers = Answers::new(&folder);
        let turn = |at: u64, web: Web| Saved {
            conversation: "pulls-c1".into(),
            at,
            question: format!("q{at}"),
            model: "haiku".into(),
            answer: "a".into(),
            web,
        };
        let first = answers.keep(turn(1, Web::Off)).unwrap();
        let later = answers.keep(turn(2, Web::Searchable)).unwrap();
        let reopened = answers.conversation("pulls-c1");
        let written = std::fs::read_to_string(folder.join("answers")).unwrap();
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(first.map(|kept| kept.web), Some(Web::Off));
        assert_eq!(later.map(|kept| kept.turns.len()), Some(2));
        assert_eq!(reopened.map(|kept| kept.web), Some(Web::Off));
        assert!(written.lines().all(|line| line.ends_with("\tno-web")));
    }
}
