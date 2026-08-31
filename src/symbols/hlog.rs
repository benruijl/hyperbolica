use std::fmt::{Display, Formatter};

use crate::core::Rat;

use super::Word;

/// The typed hyperlogarithm `Hlog[z, word]`.
///
/// An empty word denotes one; special-value evaluation is deliberately left to
/// the algebra and series layers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hlog {
    pub z: Rat,
    pub word: Word,
}

impl Hlog {
    pub fn new(z: Rat, word: Word) -> Self {
        Self { z, word }
    }

    pub fn equal(&self, other: &Self) -> bool {
        self == other
    }
}

impl Display for Hlog {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Hlog[{}, {}]", self.z, self.word)
    }
}

#[cfg(test)]
mod tests {
    use crate::core::PolyCtx;

    use super::*;

    #[test]
    fn formatting_and_equality_are_structural() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let value = Hlog::new(
            Rat::parse(ctx.clone(), "x").unwrap(),
            Word::from(vec![Rat::from_int(ctx.clone(), 0), Rat::from_int(ctx, 1)]),
        );

        assert_eq!(value.to_string(), "Hlog[x, [0, 1]]");
        assert!(value.equal(&value.clone()));
    }
}
