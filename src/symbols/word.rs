use std::fmt::{Display, Formatter};
use std::ops::{Index, IndexMut};

use crate::core::Rat;

/// A singularity of a hyperlogarithm.
///
/// HyperFLINT currently represents every letter by a rational function in the
/// ambient polynomial context.
pub type Letter = Rat;

/// An ordered list of hyperlogarithm singularities.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Word {
    pub letters: Vec<Letter>,
}

impl Word {
    pub fn new(letters: Vec<Letter>) -> Self {
        Self { letters }
    }

    pub fn len(&self) -> usize {
        self.letters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.letters.is_empty()
    }

    /// Algebraic equality of all letters in their original order.
    pub fn equal(&self, other: &Self) -> bool {
        self == other
    }

    /// A serialization key compatible with HyperFLINT's collection pass.
    ///
    /// The non-printable separator prevents ambiguities between adjacent
    /// rational-function strings. This key is only valid while all words being
    /// compared use compatible polynomial contexts, as in the upstream API.
    pub fn content_key(&self) -> String {
        let mut key = String::new();
        for letter in &self.letters {
            key.push_str(&letter.to_string());
            key.push('\u{1}');
        }
        key
    }
}

impl From<Vec<Letter>> for Word {
    fn from(letters: Vec<Letter>) -> Self {
        Self::new(letters)
    }
}

impl AsRef<[Letter]> for Word {
    fn as_ref(&self) -> &[Letter] {
        &self.letters
    }
}

impl Index<usize> for Word {
    type Output = Letter;

    fn index(&self, index: usize) -> &Self::Output {
        &self.letters[index]
    }
}

impl IndexMut<usize> for Word {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.letters[index]
    }
}

impl Display for Word {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("[")?;
        for (index, letter) in self.letters.iter().enumerate() {
            if index != 0 {
                formatter.write_str(", ")?;
            }
            Display::fmt(letter, formatter)?;
        }
        formatter.write_str("]")
    }
}

/// One rational-function coefficient times one hyperlogarithm word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WordlistTerm {
    pub coef: Rat,
    pub word: Word,
}

impl WordlistTerm {
    pub fn new(coef: Rat, word: Word) -> Self {
        Self { coef, word }
    }
}

/// A rational-function linear combination of words.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Wordlist {
    pub terms: Vec<WordlistTerm>,
}

impl Wordlist {
    pub fn new(terms: Vec<WordlistTerm>) -> Self {
        Self { terms }
    }

    pub fn len(&self) -> usize {
        self.terms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
}

impl From<Vec<WordlistTerm>> for Wordlist {
    fn from(terms: Vec<WordlistTerm>) -> Self {
        Self::new(terms)
    }
}

impl Display for Wordlist {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("{")?;
        for (index, term) in self.terms.iter().enumerate() {
            if index != 0 {
                formatter.write_str(", ")?;
            }
            write!(formatter, "{{{}, {}}}", term.coef, term.word)?;
        }
        formatter.write_str("}")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::core::PolyCtx;

    use super::*;

    fn context() -> Arc<PolyCtx> {
        PolyCtx::new(["x", "y"]).unwrap()
    }

    #[test]
    fn word_supports_ordered_access_and_upstream_formatting() {
        let ctx = context();
        let mut word = Word::from(vec![
            Rat::from_int(ctx.clone(), 0),
            Rat::from_int(ctx.clone(), 1),
        ]);

        assert_eq!(word.len(), 2);
        assert_eq!(word.to_string(), "[0, 1]");
        word[1] = Rat::parse(ctx, "x").unwrap();
        assert_eq!(word.to_string(), "[0, x]");
    }

    #[test]
    fn wordlist_format_matches_transport_vocabulary() {
        let ctx = context();
        let wordlist = Wordlist::from(vec![
            WordlistTerm::new(
                Rat::from_int(ctx.clone(), 2),
                Word::from(vec![Rat::from_int(ctx.clone(), 0)]),
            ),
            WordlistTerm::new(Rat::from_int(ctx, -1), Word::default()),
        ]);

        assert_eq!(wordlist.to_string(), "{{2, [0]}, {-1, []}}");
    }

    #[test]
    fn content_key_keeps_letter_boundaries() {
        let ctx = context();
        let two_letters = Word::from(vec![
            Rat::parse(ctx.clone(), "x").unwrap(),
            Rat::parse(ctx.clone(), "y").unwrap(),
        ]);
        let one_letter = Word::from(vec![Rat::parse(ctx, "x*y").unwrap()]);

        assert_ne!(two_letters.content_key(), one_letter.content_key());
    }
}
