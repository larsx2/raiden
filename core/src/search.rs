const START_OF_NAME: f32 = 4.0;
const START_OF_WORD: f32 = 3.0;
const NEXT_LETTER: f32 = 2.0;
const LETTER: f32 = 1.0;
const LENGTH: f32 = 0.05;
const HISTORY: f32 = 6.0;
const SAME_QUERY: f32 = 3.0;
const HALF_LIFE_DAYS: f32 = 14.0;

struct Pick {
    query: String,
    age_days: f32,
}

struct Item {
    letters: Vec<char>,
    word_starts: Vec<bool>,
    picks: Vec<Pick>,
}

pub struct Catalog {
    items: Vec<Item>,
}

fn lower(letter: char) -> char {
    letter.to_lowercase().next().unwrap_or(letter)
}

fn item(name: &str) -> Item {
    let original: Vec<char> = name.chars().collect();
    let word_starts = original
        .iter()
        .enumerate()
        .map(|(index, letter)| {
            let Some(previous) = index.checked_sub(1).map(|at| original[at]) else {
                return true;
            };
            letter.is_alphanumeric()
                && (!previous.is_alphanumeric()
                    || (previous.is_lowercase() && letter.is_uppercase())
                    || (previous.is_alphabetic() && letter.is_numeric()))
        })
        .collect();
    Item {
        letters: original.into_iter().map(lower).collect(),
        word_starts,
        picks: Vec::new(),
    }
}

fn query_letters(query: &str) -> Vec<char> {
    query
        .chars()
        .filter(|letter| !letter.is_whitespace())
        .map(lower)
        .collect()
}

fn bonus(item: &Item, at: usize) -> f32 {
    if item.word_starts[at] {
        START_OF_WORD
    } else {
        0.0
    }
}

fn first_scores(item: &Item, first: char) -> Vec<f32> {
    (0..item.letters.len())
        .map(|at| {
            if item.letters[at] == first {
                LETTER + bonus(item, at) + if at == 0 { START_OF_NAME } else { 0.0 }
            } else {
                f32::NEG_INFINITY
            }
        })
        .collect()
}

fn next_scores(item: &Item, scores: &[f32], letter: char, next: &mut [f32]) {
    let mut best_before = f32::NEG_INFINITY;
    for at in 0..item.letters.len() {
        next[at] = f32::NEG_INFINITY;
        if item.letters[at] == letter {
            let follows = at
                .checked_sub(1)
                .map_or(f32::NEG_INFINITY, |previous| scores[previous] + NEXT_LETTER);
            let jumps = if item.word_starts[at] {
                best_before
            } else {
                f32::NEG_INFINITY
            };
            next[at] = follows.max(jumps) + LETTER + bonus(item, at);
        }
        best_before = best_before.max(scores[at]);
    }
}

fn best_at(scores: &[f32]) -> Option<usize> {
    let best = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    if best.is_finite() {
        scores.iter().position(|&score| score == best)
    } else {
        None
    }
}

fn match_score(item: &Item, query: &[char]) -> Option<f32> {
    let Some((first, rest)) = query.split_first() else {
        return Some(0.0);
    };
    let mut scores = first_scores(item, *first);
    let mut next = vec![f32::NEG_INFINITY; scores.len()];
    for letter in rest {
        next_scores(item, &scores, *letter, &mut next);
        std::mem::swap(&mut scores, &mut next);
    }
    let best = scores.into_iter().fold(f32::NEG_INFINITY, f32::max);
    best.is_finite()
        .then_some(best - LENGTH * item.letters.len() as f32)
}

fn match_positions(item: &Item, query: &[char]) -> Vec<usize> {
    let Some((first, rest)) = query.split_first() else {
        return Vec::new();
    };
    let mut rows = vec![first_scores(item, *first)];
    for letter in rest {
        let mut next = vec![f32::NEG_INFINITY; item.letters.len()];
        next_scores(item, &rows[rows.len() - 1], *letter, &mut next);
        rows.push(next);
    }
    let Some(mut at) = rows.last().and_then(|last| best_at(last)) else {
        return Vec::new();
    };
    let mut found = vec![at];
    for previous in rows.iter().rev().skip(1) {
        let follows = at
            .checked_sub(1)
            .map_or(f32::NEG_INFINITY, |before| previous[before] + NEXT_LETTER);
        let jump = if item.word_starts[at] {
            best_at(&previous[..at]).filter(|&before| previous[before] > follows)
        } else {
            None
        };
        at = jump.unwrap_or(at.saturating_sub(1));
        found.push(at);
    }
    found.reverse();
    found
}

fn learned(item: &Item, query: &str) -> f32 {
    let query = query.trim().to_lowercase();
    let weight: f32 = item
        .picks
        .iter()
        .map(|pick| {
            let recency = 0.5_f32.powf(pick.age_days.max(0.0) / HALF_LIFE_DAYS);
            let same = !query.is_empty()
                && !pick.query.is_empty()
                && (pick.query.starts_with(&query) || query.starts_with(&pick.query));
            recency * if same { SAME_QUERY } else { 1.0 }
        })
        .sum();
    HISTORY * weight.ln_1p()
}

impl Catalog {
    pub fn new<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            items: names.into_iter().map(item).collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn remember(&mut self, index: usize, query: &str, age_days: f32) {
        if let Some(item) = self.items.get_mut(index) {
            item.picks.push(Pick {
                query: query.trim().to_lowercase(),
                age_days,
            });
        }
    }

    /// ```
    /// use raiden_core::search::Catalog;
    /// let catalog = Catalog::new(["Safari", "System Settings", "Slack"]);
    /// assert_eq!(catalog.rank("saf"), [0]);
    /// assert!(catalog.rank("sen").is_empty());
    /// ```
    pub fn rank(&self, query: &str) -> Vec<u32> {
        let letters = query_letters(query);
        let mut found: Vec<(f32, u32)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let score = match_score(item, &letters)?;
                let score = if letters.is_empty() { 0.0 } else { score };
                Some((score + learned(item, query), u32::try_from(index).ok()?))
            })
            .collect();
        found.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        found.into_iter().map(|(_, index)| index).collect()
    }

    /// ```
    /// use raiden_core::search::Catalog;
    /// let catalog = Catalog::new(["Safari", "System Settings"]);
    /// assert_eq!(catalog.positions(1, "sys set"), [0, 1, 2, 7, 8, 9]);
    /// assert!(catalog.positions(0, "sen").is_empty());
    /// ```
    pub fn positions(&self, index: usize, query: &str) -> Vec<u32> {
        let Some(item) = self.items.get(index) else {
            return Vec::new();
        };
        match_positions(item, &query_letters(query))
            .into_iter()
            .filter_map(|at| u32::try_from(at).ok())
            .collect()
    }

    /// ```
    /// use raiden_core::search::Catalog;
    /// let mut catalog = Catalog::new(["Safari", "Slack", "Zed"]);
    /// catalog.remember(2, "z", 1.0);
    /// catalog.remember(2, "", 2.0);
    /// catalog.remember(0, "", 1.0);
    /// assert_eq!(catalog.suggestions(5), [2, 0]);
    /// assert_eq!(catalog.suggestions(1), [2]);
    /// ```
    pub fn suggestions(&self, limit: usize) -> Vec<u32> {
        let mut used: Vec<(f32, u32)> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.picks.is_empty())
            .filter_map(|(index, item)| Some((learned(item, ""), u32::try_from(index).ok()?)))
            .collect();
        used.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        used.into_iter()
            .take(limit)
            .map(|(_, index)| index)
            .collect()
    }

    /// ```
    /// use raiden_core::search::Catalog;
    /// let catalog = Catalog::from_text("Safari\nSlack\n\n1\t2.5\tsl\n");
    /// assert_eq!(catalog.len(), 2);
    /// assert_eq!(catalog.rank(""), [1, 0]);
    /// ```
    pub fn from_text(text: &str) -> Self {
        let (names, picks) = text.split_once("\n\n").unwrap_or((text, ""));
        let names = if names.trim().is_empty() { "" } else { names };
        let mut catalog = Self::new(names.lines());
        for line in picks.lines() {
            let mut fields = line.splitn(3, '\t');
            let (Some(index), Some(age), Some(query)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            if let (Ok(index), Ok(age)) = (index.parse(), age.parse()) {
                catalog.remember(index, query, age);
            }
        }
        catalog
    }
}

pub fn catalog_text<'a>(
    names: impl IntoIterator<Item = &'a str>,
    picks: impl IntoIterator<Item = (usize, f32, &'a str)>,
) -> String {
    let flat = |text: &str| text.replace(['\n', '\r', '\t'], " ");
    let mut text: String = names.into_iter().map(|name| flat(name) + "\n").collect();
    text.push('\n');
    for (index, age_days, query) in picks {
        text.push_str(&format!("{index}\t{age_days}\t{}\n", flat(query)));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(catalog: &Catalog, list: &[&str], query: &str) -> Vec<String> {
        catalog
            .rank(query)
            .into_iter()
            .map(|index| list[index as usize].to_owned())
            .collect()
    }

    #[test]
    fn letters_must_follow_each_other_or_start_words() {
        let list = [
            "Safari",
            "System Settings",
            "Visual Studio Code",
            "TextEdit",
        ];
        let catalog = Catalog::new(list);
        assert_eq!(names(&catalog, &list, "saf"), ["Safari"]);
        assert!(names(&catalog, &list, "sen").is_empty());
        assert_eq!(names(&catalog, &list, "vsc"), ["Visual Studio Code"]);
        assert_eq!(names(&catalog, &list, "te")[0], "TextEdit");
        assert_eq!(names(&catalog, &list, "ted"), ["TextEdit"]);
        assert_eq!(names(&catalog, &list, "sys set"), ["System Settings"]);
    }

    #[test]
    fn a_start_of_name_beats_a_later_word() {
        let list = ["Font Book", "Books"];
        let catalog = Catalog::new(list);
        assert_eq!(names(&catalog, &list, "bo"), ["Books", "Font Book"]);
    }

    #[test]
    fn picks_lift_an_app_and_fade_with_time() {
        let list = ["Safari", "Slack", "Spotify"];
        let mut catalog = Catalog::new(list);
        assert_ne!(names(&catalog, &list, "s")[0], "Spotify");
        catalog.remember(2, "s", 1.0);
        assert_eq!(names(&catalog, &list, "s")[0], "Spotify");
        let mut old = Catalog::new(list);
        old.remember(2, "s", 1.0);
        old.remember(1, "s", 1.0);
        old.remember(1, "s", 0.5);
        old.remember(2, "s", 365.0);
        assert_eq!(names(&old, &list, "s")[0], "Slack");
    }

    #[test]
    fn a_pick_for_the_same_query_counts_more() {
        let list = ["Slack", "Spotify"];
        let mut catalog = Catalog::new(list);
        catalog.remember(0, "music", 1.0);
        catalog.remember(0, "chat", 1.0);
        catalog.remember(1, "sp", 1.0);
        assert_eq!(names(&catalog, &list, "s")[0], "Spotify");
    }

    #[test]
    fn text_round_trip_keeps_names_and_picks() {
        let text = catalog_text(["Safari", "Sl\tack"], [(1, 3.0, "s\nl")]);
        let catalog = Catalog::from_text(&text);
        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog.rank(""), [1, 0]);
    }

    #[test]
    fn empty_catalog_ranks_nothing() {
        assert!(Catalog::from_text("").rank("a").is_empty());
        let none: [&str; 0] = [];
        assert!(Catalog::from_text(&catalog_text(none, [])).is_empty());
    }

    #[test]
    fn a_pick_with_nothing_typed_is_not_a_related_query() {
        let list = ["Slack", "Spotify"];
        let mut catalog = Catalog::new(list);
        for _ in 0..3 {
            catalog.remember(0, "", 1.0);
        }
        catalog.remember(1, "sp", 1.0);
        catalog.remember(1, "sp", 1.0);
        assert_eq!(names(&catalog, &list, "s")[0], "Spotify");
    }

    fn bold(catalog: &Catalog, list: &[&str], index: usize, query: &str) -> String {
        let marked = catalog.positions(index, query);
        list[index]
            .chars()
            .enumerate()
            .map(|(at, letter)| {
                let at = u32::try_from(at).unwrap_or(u32::MAX);
                if marked.contains(&at) {
                    letter.to_uppercase().collect()
                } else {
                    letter.to_lowercase().collect::<String>()
                }
            })
            .collect()
    }

    #[test]
    fn positions_mark_the_letters_the_ranking_matched() {
        let list = [
            "Safari",
            "System Settings",
            "Visual Studio Code",
            "TextEdit",
            "Font Book",
        ];
        let catalog = Catalog::new(list);
        assert_eq!(bold(&catalog, &list, 0, "saf"), "SAFari");
        assert_eq!(bold(&catalog, &list, 1, "sys set"), "SYStem SETtings");
        assert_eq!(bold(&catalog, &list, 1, "ss"), "System Settings");
        assert_eq!(bold(&catalog, &list, 2, "vsc"), "Visual Studio Code");
        assert_eq!(bold(&catalog, &list, 3, "ted"), "TextEDit");
        assert_eq!(bold(&catalog, &list, 4, "bo"), "font BOok");
        assert_eq!(bold(&catalog, &list, 0, "a"), "sAfari");
    }

    #[test]
    fn positions_are_empty_without_a_match_a_query_or_an_item() {
        let catalog = Catalog::new(["Safari"]);
        assert!(catalog.positions(0, "sen").is_empty());
        assert!(catalog.positions(0, "").is_empty());
        assert!(catalog.positions(0, "   ").is_empty());
        assert!(catalog.positions(3, "s").is_empty());
    }

    #[test]
    fn positions_count_letters_not_bytes() {
        let catalog = Catalog::new(["Café Olé"]);
        assert_eq!(catalog.positions(0, "éo"), [3, 5]);
    }

    #[test]
    fn suggestions_list_only_used_items_most_used_first() {
        let mut catalog = Catalog::new(["Safari", "Slack", "Spotify", "Zed"]);
        assert!(catalog.suggestions(5).is_empty());
        catalog.remember(1, "", 1.0);
        catalog.remember(3, "z", 1.0);
        catalog.remember(3, "zed", 1.0);
        catalog.remember(0, "", 300.0);
        assert_eq!(catalog.suggestions(5), [3, 1, 0]);
        assert_eq!(catalog.suggestions(2), [3, 1]);
    }
}
