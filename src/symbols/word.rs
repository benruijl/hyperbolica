use std::fmt::{Display, Formatter};
use std::ops::{Index, IndexMut};

use crate::core::Rat;

/// A singularity of a hyperlogarithm.
///
/// HyperFLINT currently represents every letter by a rational function in the
/// ambient polynomial context.
pub type Letter = Rat;

/// An ordered list of hyperlogarithm singularities.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
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

    /// A legacy formatting key compatible with HyperFLINT's collection pass.
    ///
    /// The non-printable separator prevents ambiguities between adjacent
    /// rational-function strings. Semantic lookup must use the structural
    /// [`Eq`] and [`Hash`](std::hash::Hash) implementations on `Word`; this
    /// spelling remains temporarily for transport/ordering callers that are
    /// migrated in a separate pass.
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
    use std::hash::{Hash, Hasher};
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

    #[derive(Default)]
    struct RecordingHasher(Vec<u8>);

    impl Hasher for RecordingHasher {
        fn finish(&self) -> u64 {
            0
        }

        fn write(&mut self, bytes: &[u8]) {
            self.0.extend_from_slice(bytes);
        }
    }

    fn recorded_hash(value: &impl Hash) -> Vec<u8> {
        let mut hasher = RecordingHasher::default();
        value.hash(&mut hasher);
        hasher.0
    }

    #[test]
    fn rat_and_word_hashes_follow_canonical_value_and_full_context_equality() {
        let ctx = context();
        let cancelled = Rat::parse(ctx.clone(), "(x^2-y^2)/(x-y)").unwrap();
        let canonical = Rat::parse(ctx.clone(), "x+y").unwrap();
        assert_eq!(cancelled, canonical);
        assert_eq!(recorded_hash(&cancelled), recorded_hash(&canonical));

        // Native constant-polynomial equality intentionally ignores variable
        // maps. Rat equality does not, so its hash must include the context.
        let x_constant = Rat::one(PolyCtx::new(["x"]).unwrap());
        let y_constant = Rat::one(PolyCtx::new(["y"]).unwrap());
        assert_ne!(x_constant, y_constant);
        assert_ne!(recorded_hash(&x_constant), recorded_hash(&y_constant));

        let first = Word::new(vec![cancelled]);
        let second = Word::new(vec![canonical]);
        assert_eq!(first, second);
        assert_eq!(recorded_hash(&first), recorded_hash(&second));
    }

    #[test]
    fn structurally_distinct_words_can_have_identical_legacy_keys() {
        let x_ctx = PolyCtx::new(["x"]).unwrap();
        let y_ctx = PolyCtx::new(["y"]).unwrap();
        let x_word = Word::new(vec![Rat::one(x_ctx)]);
        let y_word = Word::new(vec![Rat::one(y_ctx)]);
        assert_eq!(x_word.to_string(), y_word.to_string());
        assert_eq!(x_word.content_key(), y_word.content_key());
        assert_ne!(x_word, y_word);
    }
}
