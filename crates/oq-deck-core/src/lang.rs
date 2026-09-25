//! The language an explanation is written in.
//!
//! The console's interface is in Chinese or English, and the sentences
//! this crate writes for an operator — why a capability is off, what an
//! attribution is missing — are part of that interface. Each is written
//! in both at the place it is produced, and the caller says which one
//! the reader asked for.

use serde::{Deserialize, Serialize};

/// Chinese or English. Chinese when nothing says otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    Zh,
    En,
}

/// A sentence for a person, written in both languages where it is worded.
///
/// Whoever words a sentence knows what it means; a reader arriving later
/// does not, so the pair is made at the place it is written and carried
/// from there. Records keep both — an incident read next month is read in
/// the language of whoever is looking at it then, not the one the person
/// on call that night happened to use.
///
/// What is *not* a sentence in this sense is data: a path, a symbol, a
/// venue's own error text. [`Said::same`] says so out loud, rather than
/// leaving a reader to guess whether the other language is missing.
///
/// On the wire it is the two renderings under their own names, so a
/// reader never has to be told which one it is holding.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Said {
    pub zh: String,
    pub en: String,
}

impl Said {
    #[must_use]
    pub fn new(zh: impl Into<String>, en: impl Into<String>) -> Self {
        Self {
            zh: zh.into(),
            en: en.into(),
        }
    }

    /// The same words in both languages: a name, a path, an error from a
    /// process that does not know this crate exists.
    #[must_use]
    pub fn same(s: impl Into<String>) -> Self {
        let s = s.into();
        Self {
            zh: s.clone(),
            en: s,
        }
    }

    /// This sentence in `lang`.
    #[must_use]
    pub fn in_lang(&self, lang: Lang) -> &str {
        lang.pick(self.zh.as_str(), self.en.as_str())
    }
}

impl From<String> for Said {
    fn from(s: String) -> Self {
        Self::same(s)
    }
}

impl From<&str> for Said {
    fn from(s: &str) -> Self {
        Self::same(s)
    }
}

impl Lang {
    /// The language an `Accept-Language` header asks for: the first of
    /// Chinese or English it names, in the order written. Weights are
    /// not honoured: the interface sends one tag, and anything else is a
    /// person's browser, whose first preference is the one to follow.
    #[must_use]
    pub fn from_accept_language(header: &str) -> Self {
        for tag in header.split(',') {
            let primary = tag
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if primary == "zh" || primary.starts_with("zh-") {
                return Self::Zh;
            }
            if primary == "en" || primary.starts_with("en-") {
                return Self::En;
            }
        }
        Self::Zh
    }

    /// One of two renderings, in this language.
    #[must_use]
    pub fn pick<T>(self, zh: T, en: T) -> T {
        match self {
            Self::Zh => zh,
            Self::En => en,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Lang, Said};

    #[test]
    fn the_first_named_language_wins() {
        assert_eq!(Lang::from_accept_language("en"), Lang::En);
        assert_eq!(
            Lang::from_accept_language("en-US,en;q=0.9,zh;q=0.8"),
            Lang::En
        );
        assert_eq!(Lang::from_accept_language("zh-CN"), Lang::Zh);
        assert_eq!(Lang::from_accept_language("fr, zh-TW;q=0.5, en"), Lang::Zh);
        assert_eq!(Lang::from_accept_language("fr"), Lang::Zh);
        assert_eq!(Lang::from_accept_language(""), Lang::Zh);
        // A tag that only starts with the letters is another language.
        assert_eq!(Lang::from_accept_language("eno, en"), Lang::En);
    }

    #[test]
    fn a_sentence_reads_back_in_either_language() {
        let s = Said::new("请先登录。", "Sign in first.");
        assert_eq!(s.in_lang(Lang::Zh), "请先登录。");
        assert_eq!(s.in_lang(Lang::En), "Sign in first.");
        // Data has one rendering, and says so rather than half-answering.
        let d = Said::same("oqp-live.service");
        assert_eq!(d.in_lang(Lang::Zh), d.in_lang(Lang::En));
        // A bare error from elsewhere becomes one of these without a
        // second wording being invented for it.
        assert_eq!(Said::from("no such file".to_string()).en, "no such file");
    }
}
