//! The language an explanation is written in.
//!
//! The console's interface is in Chinese or English, and the sentences
//! this crate writes for an operator — why a capability is off, what an
//! attribution is missing — are part of that interface. Each is written
//! in both at the place it is produced, and the caller says which one
//! the reader asked for.

/// Chinese or English. Chinese when nothing says otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    Zh,
    En,
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
    use super::Lang;

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
}
