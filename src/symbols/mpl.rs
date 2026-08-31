use std::fmt::{Display, Formatter};

use crate::core::Rat;

/// A typed multiple polylogarithm `Mpl[{n1,...}, {z1,...}]`.
///
/// Like the C++ value type, this struct does not enforce equal vector lengths;
/// individual algorithms validate the shapes they require.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mpl {
    pub indices: Vec<i64>,
    pub args: Vec<Rat>,
}

impl Mpl {
    pub fn new(indices: Vec<i64>, args: Vec<Rat>) -> Self {
        Self { indices, args }
    }

    pub fn equal(&self, other: &Self) -> bool {
        self == other
    }
}

impl Display for Mpl {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Mpl[{")?;
        for (index, weight) in self.indices.iter().enumerate() {
            if index != 0 {
                formatter.write_str(",")?;
            }
            Display::fmt(weight, formatter)?;
        }
        formatter.write_str("},{")?;
        for (index, argument) in self.args.iter().enumerate() {
            if index != 0 {
                formatter.write_str(",")?;
            }
            Display::fmt(argument, formatter)?;
        }
        formatter.write_str("}]")
    }
}

#[cfg(test)]
mod tests {
    use crate::core::PolyCtx;

    use super::*;

    #[test]
    fn formatting_matches_hyperflint() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let value = Mpl::new(
            vec![2, -1],
            vec![Rat::parse(ctx.clone(), "x").unwrap(), Rat::from_int(ctx, 1)],
        );

        assert_eq!(value.to_string(), "Mpl[{2,-1},{x,1}]");
        assert!(value.equal(&value.clone()));
    }

    #[test]
    fn empty_mpl_is_a_valid_symbolic_unit() {
        let value = Mpl::new(Vec::new(), Vec::new());

        assert_eq!(value.to_string(), "Mpl[{},{}]");
        assert!(value.equal(&Mpl::new(Vec::new(), Vec::<Rat>::new())));
    }
}
