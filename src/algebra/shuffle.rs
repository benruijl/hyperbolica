use std::collections::HashMap;

use crate::symbols::{Letter, Word, Wordlist, WordlistTerm};

/// Shuffle two words, collecting identical interleavings.
///
/// The all-empty case follows HyperFLINT and returns an empty wordlist because
/// no ambient polynomial context is available from which to construct the
/// unit coefficient. [`shuffle_product`] handles that case when coefficients
/// provide a context.
pub fn shuffle_words(left: &Word, right: &Word) -> Wordlist {
    if left.is_empty() && right.is_empty() {
        return Wordlist::default();
    }

    let coefficient_context = if left.is_empty() {
        right[0].ctx().clone()
    } else {
        left[0].ctx().clone()
    };
    let unit = crate::core::Rat::one(coefficient_context);
    let mut words = Vec::new();
    let mut prefix = Vec::with_capacity(left.len() + right.len());
    shuffle_interleavings(&left.letters, &right.letters, &mut prefix, &mut words);

    collect_words(&Wordlist::new(
        words
            .into_iter()
            .map(|word| WordlistTerm::new(unit.clone(), word))
            .collect(),
    ))
}

fn shuffle_interleavings(
    left: &[Letter],
    right: &[Letter],
    prefix: &mut Vec<Letter>,
    output: &mut Vec<Word>,
) {
    if left.is_empty() {
        let prefix_len = prefix.len();
        prefix.extend_from_slice(right);
        output.push(Word::from(prefix.clone()));
        prefix.truncate(prefix_len);
        return;
    }
    if right.is_empty() {
        let prefix_len = prefix.len();
        prefix.extend_from_slice(left);
        output.push(Word::from(prefix.clone()));
        prefix.truncate(prefix_len);
        return;
    }

    prefix.push(left[0].clone());
    shuffle_interleavings(&left[1..], right, prefix, output);
    prefix.pop();

    prefix.push(right[0].clone());
    shuffle_interleavings(left, &right[1..], prefix, output);
    prefix.pop();
}

/// Bilinearly shuffle two rational-function combinations of words.
pub fn shuffle_product(left: &Wordlist, right: &Wordlist) -> Wordlist {
    let mut output = Wordlist::default();

    for left_term in &left.terms {
        for right_term in &right.terms {
            let pair_coefficient = &left_term.coef * &right_term.coef;
            if left_term.word.is_empty() && right_term.word.is_empty() {
                output
                    .terms
                    .push(WordlistTerm::new(pair_coefficient, Word::default()));
                continue;
            }

            let shuffled = shuffle_words(&left_term.word, &right_term.word);
            output.terms.extend(
                shuffled
                    .terms
                    .into_iter()
                    .map(|term| WordlistTerm::new(&pair_coefficient * &term.coef, term.word)),
            );
        }
    }

    collect_words(&output)
}

/// Multiply wordlists by concatenating every pair of words.
pub fn concat_mul(left: &Wordlist, right: &Wordlist) -> Wordlist {
    let mut output = Wordlist::default();

    for left_term in &left.terms {
        for right_term in &right.terms {
            let mut letters = Vec::with_capacity(left_term.word.len() + right_term.word.len());
            letters.extend(left_term.word.letters.iter().cloned());
            letters.extend(right_term.word.letters.iter().cloned());
            output.terms.push(WordlistTerm::new(
                &left_term.coef * &right_term.coef,
                Word::from(letters),
            ));
        }
    }

    collect_words(&output)
}

/// Scale every coefficient, without collecting or dropping zero terms.
pub fn scalar_mul_wordlist(wordlist: &Wordlist, scalar: &crate::core::Rat) -> Wordlist {
    Wordlist::new(
        wordlist
            .terms
            .iter()
            .map(|term| WordlistTerm::new(&term.coef * scalar, term.word.clone()))
            .collect(),
    )
}

/// Merge identical words, preserving their first-occurrence order.
pub fn collect_words(wordlist: &Wordlist) -> Wordlist {
    let mut indices = HashMap::<String, usize>::new();
    let mut kept = Vec::<WordlistTerm>::new();

    for term in &wordlist.terms {
        let key = term.word.content_key();
        if let Some(&index) = indices.get(&key) {
            kept[index].coef = &kept[index].coef + &term.coef;
        } else {
            indices.insert(key, kept.len());
            kept.push(term.clone());
        }
    }

    Wordlist::new(
        kept.into_iter()
            .filter(|term| !term.coef.is_zero())
            .collect(),
    )
}

/// Strip consecutive leading letters whose canonical string is `reg_letter`.
pub fn reg_head(word: &Word, reg_letter: &str) -> Word {
    let first_kept = word
        .letters
        .iter()
        .position(|letter| letter.to_string() != reg_letter)
        .unwrap_or(word.len());
    Word::from(word.letters[first_kept..].to_vec())
}

/// Strip consecutive trailing letters whose canonical string is `reg_letter`.
pub fn reg_tail(word: &Word, reg_letter: &str) -> Word {
    let kept_len = word
        .letters
        .iter()
        .rposition(|letter| letter.to_string() != reg_letter)
        .map_or(0, |index| index + 1);
    Word::from(word.letters[..kept_len].to_vec())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::core::{PolyCtx, Rat};

    use super::*;

    fn context() -> Arc<PolyCtx> {
        PolyCtx::new(["x"]).unwrap()
    }

    fn integer(ctx: &Arc<PolyCtx>, value: i64) -> Rat {
        Rat::from_int(ctx.clone(), value)
    }

    fn word(ctx: &Arc<PolyCtx>, letters: &[i64]) -> Word {
        Word::from(
            letters
                .iter()
                .map(|&letter| integer(ctx, letter))
                .collect::<Vec<_>>(),
        )
    }

    fn term(coef: i64, word: Word, ctx: &Arc<PolyCtx>) -> WordlistTerm {
        WordlistTerm::new(integer(ctx, coef), word)
    }

    #[test]
    fn plain_empty_shuffle_is_unseeded_but_one_sided_empty_has_a_unit() {
        let ctx = context();
        let empty = Word::default();
        let nonempty = word(&ctx, &[0, 1]);

        assert!(shuffle_words(&empty, &empty).is_empty());
        assert_eq!(
            shuffle_words(&empty, &nonempty),
            Wordlist::from(vec![term(1, nonempty.clone(), &ctx)])
        );
        assert_eq!(
            shuffle_words(&nonempty, &empty),
            Wordlist::from(vec![term(1, nonempty, &ctx)])
        );
    }

    #[test]
    fn shuffle_preserves_recurrence_order() {
        let ctx = context();
        let actual = shuffle_words(&word(&ctx, &[0]), &word(&ctx, &[1, 2]));
        let expected = Wordlist::from(vec![
            term(1, word(&ctx, &[0, 1, 2]), &ctx),
            term(1, word(&ctx, &[1, 0, 2]), &ctx),
            term(1, word(&ctx, &[1, 2, 0]), &ctx),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn repeated_interleavings_are_collected_with_multiplicity() {
        let ctx = context();
        assert_eq!(
            shuffle_words(&word(&ctx, &[0]), &word(&ctx, &[0])),
            Wordlist::from(vec![term(2, word(&ctx, &[0, 0]), &ctx)])
        );
    }

    #[test]
    fn wordlist_shuffle_is_bilinear_and_handles_empty_word_units() {
        let ctx = context();
        let left = Wordlist::from(vec![
            term(2, word(&ctx, &[0]), &ctx),
            term(3, word(&ctx, &[1]), &ctx),
        ]);
        let right = Wordlist::from(vec![term(5, word(&ctx, &[0]), &ctx)]);
        assert_eq!(
            shuffle_product(&left, &right),
            Wordlist::from(vec![
                term(20, word(&ctx, &[0, 0]), &ctx),
                term(15, word(&ctx, &[1, 0]), &ctx),
                term(15, word(&ctx, &[0, 1]), &ctx),
            ])
        );

        let empty_left = Wordlist::from(vec![term(2, Word::default(), &ctx)]);
        let empty_right = Wordlist::from(vec![term(3, Word::default(), &ctx)]);
        assert_eq!(
            shuffle_product(&empty_left, &empty_right),
            Wordlist::from(vec![term(6, Word::default(), &ctx)])
        );
    }

    #[test]
    fn concatenation_collects_duplicate_words() {
        let ctx = context();
        let left = Wordlist::from(vec![
            term(2, word(&ctx, &[0]), &ctx),
            term(3, word(&ctx, &[0]), &ctx),
        ]);
        let right = Wordlist::from(vec![term(4, word(&ctx, &[1]), &ctx)]);

        assert_eq!(
            concat_mul(&left, &right),
            Wordlist::from(vec![term(20, word(&ctx, &[0, 1]), &ctx)])
        );
    }

    #[test]
    fn collection_preserves_first_occurrence_and_drops_cancellation() {
        let ctx = context();
        let a = word(&ctx, &[0]);
        let b = word(&ctx, &[1]);
        let input = Wordlist::from(vec![
            term(1, a.clone(), &ctx),
            term(2, b.clone(), &ctx),
            term(-1, a, &ctx),
            term(3, b.clone(), &ctx),
        ]);

        assert_eq!(
            collect_words(&input),
            Wordlist::from(vec![term(5, b, &ctx)])
        );
    }

    #[test]
    fn scalar_multiplication_defers_zero_elimination() {
        let ctx = context();
        let input = Wordlist::from(vec![term(4, word(&ctx, &[0]), &ctx)]);
        let scaled = scalar_mul_wordlist(&input, &integer(&ctx, 0));

        assert_eq!(scaled.len(), 1);
        assert!(scaled.terms[0].coef.is_zero());
        assert!(collect_words(&scaled).is_empty());
    }

    #[test]
    fn regularizer_stripping_only_removes_the_selected_edge_run() {
        let ctx = context();
        let input = word(&ctx, &[0, 0, 1, 0]);

        assert_eq!(reg_head(&input, "0"), word(&ctx, &[1, 0]));
        assert_eq!(reg_tail(&input, "0"), word(&ctx, &[0, 0, 1]));
        assert_eq!(reg_head(&word(&ctx, &[0, 0]), "0"), Word::default());
        assert_eq!(reg_tail(&Word::default(), "0"), Word::default());
    }
}
