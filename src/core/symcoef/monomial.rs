use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter, Write};

use super::SymMonomial;
use crate::core::Rat;

impl SymMonomial {
    pub fn new(prefactor: Rat) -> Self {
        Self {
            prefactor,
            pi_power: 0,
            i_power: 0,
            log_powers: BTreeMap::new(),
            delta_powers: BTreeMap::new(),
            period_powers: BTreeMap::new(),
        }
    }

    /// A stable representation of the symbolic powers, excluding the
    /// rational prefactor.
    pub fn power_key(&self) -> String {
        let mut key = format!("P{}|I{}|L", self.pi_power, self.i_power);
        for (argument, exponent) in &self.log_powers {
            let _ = write!(key, "{argument}:{exponent},");
        }
        key.push_str("|D");
        for (variable, exponent) in &self.delta_powers {
            let _ = write!(key, "{variable}:{exponent},");
        }
        key.push_str("|Q");
        for (period, exponent) in &self.period_powers {
            let _ = write!(key, "{period}:{exponent},");
        }
        key
    }

    pub fn is_pure_rat(&self) -> bool {
        self.pi_power == 0
            && self.i_power == 0
            && self.log_powers.is_empty()
            && self.delta_powers.is_empty()
            && self.period_powers.is_empty()
    }

    pub(super) fn powers_cmp(&self, other: &Self) -> Ordering {
        self.pi_power
            .cmp(&other.pi_power)
            .then_with(|| self.i_power.cmp(&other.i_power))
            .then_with(|| self.log_powers.cmp(&other.log_powers))
            .then_with(|| self.delta_powers.cmp(&other.delta_powers))
            .then_with(|| self.period_powers.cmp(&other.period_powers))
    }

    pub(super) fn same_powers(&self, other: &Self) -> bool {
        self.powers_cmp(other) == Ordering::Equal
    }

    pub(super) fn normalize(&mut self) {
        // Reduce modulo four first so negative powers have the expected
        // algebraic meaning too: I^-1 = -I and I^-2 = -1.
        let residue = self.i_power.rem_euclid(4);
        if residue >= 2 {
            self.prefactor = self.prefactor.negated();
        }
        self.i_power = residue % 2;

        self.log_powers.retain(|_, exponent| *exponent != 0);
        self.period_powers.retain(|_, exponent| *exponent != 0);
        self.delta_powers.retain(|_, exponent| {
            *exponent = exponent.rem_euclid(2);
            *exponent != 0
        });
    }
}

impl Display for SymMonomial {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "({})", self.prefactor)?;

        match self.pi_power {
            0 => {}
            1 => formatter.write_str("*Pi")?,
            exponent => write!(formatter, "*Pi^{exponent}")?,
        }
        match self.i_power {
            0 => {}
            1 => formatter.write_str("*I")?,
            exponent => write!(formatter, "*I^{exponent}")?,
        }
        for (argument, exponent) in &self.log_powers {
            write!(formatter, "*Log[{argument}]")?;
            if *exponent != 1 {
                write!(formatter, "^{exponent}")?;
            }
        }
        for (variable, exponent) in &self.delta_powers {
            let name = self
                .prefactor
                .ctx()
                .vars()
                .get(*variable)
                .map(String::as_str)
                .unwrap_or("<invalid>");
            write!(formatter, "*delta[{name}]")?;
            if *exponent != 1 {
                write!(formatter, "^{exponent}")?;
            }
        }
        for (period, exponent) in &self.period_powers {
            write!(formatter, "*Period[{period}]")?;
            if *exponent != 1 {
                write!(formatter, "^{exponent}")?;
            }
        }
        Ok(())
    }
}
