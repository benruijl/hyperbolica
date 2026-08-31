use crate::core::Rat;
use crate::symbols::Word;

/// The symbolic expression subset accepted by `ConvertToHlogRegInf`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Leaf(Rat),
    Plus(Vec<Expr>),
    Times(Vec<Expr>),
    Power(Box<Expr>, i64),
    Hlog { arg: Rat, word: Word },
}

impl Expr {
    pub fn leaf(value: Rat) -> Self {
        Self::Leaf(value)
    }

    pub fn plus(children: Vec<Self>) -> Self {
        Self::Plus(children)
    }

    pub fn times(children: Vec<Self>) -> Self {
        Self::Times(children)
    }

    pub fn power(base: Self, exponent: i64) -> crate::Result<Self> {
        if exponent < 1 {
            return Err(crate::Error::InvalidExponent(exponent));
        }
        Ok(Self::Power(Box::new(base), exponent))
    }

    pub fn hlog(arg: Rat, word: Word) -> Self {
        Self::Hlog { arg, word }
    }

    pub fn canonical_string(&self) -> String {
        match self {
            Self::Leaf(value) => value.to_string(),
            Self::Plus(children) | Self::Times(children) => {
                let mut parts = children
                    .iter()
                    .map(Self::canonical_string)
                    .collect::<Vec<_>>();
                parts.sort_unstable();
                let head = if matches!(self, Self::Plus(_)) {
                    "Plus"
                } else {
                    "Times"
                };
                format!("{head}[{}]", parts.join(","))
            }
            Self::Power(base, exponent) => {
                format!("Power[{},{}]", base.canonical_string(), exponent)
            }
            Self::Hlog { arg, word } => format!(
                "Hlog[{},[{}]]",
                arg,
                word.letters
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        }
    }

    pub fn as_leaf(&self) -> Option<&Rat> {
        match self {
            Self::Leaf(value) => Some(value),
            _ => None,
        }
    }
}

impl std::fmt::Display for Expr {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.canonical_string())
    }
}
