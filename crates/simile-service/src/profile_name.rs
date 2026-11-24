//! Human-readable names for profiles created without an explicit `--profile`.

mod words;

use std::collections::BTreeSet;

use words::{ADJECTIVES, NOUNS};

const RANDOM_ATTEMPTS: usize = 64;

/// Generate an unused `adjective-(noun|adjective)` profile name.
///
/// Random selection keeps independently created profiles visually distinct. An
/// exhaustive pass guarantees that random collisions cannot report exhaustion
/// while any catalog combination remains available.
pub(crate) fn generate_with_rng<R>(rng: &mut R, existing: &BTreeSet<String>) -> Option<String>
where
    R: rand::Rng + ?Sized,
{
    let combinations = ADJECTIVES.len() * second_word_count();
    for _ in 0..RANDOM_ATTEMPTS {
        let index = rng.random_range(0..combinations);
        if let Some(candidate) = candidate(index) {
            if !existing.contains(&candidate) {
                return Some(candidate);
            }
        }
    }

    let start = rng.random_range(0..combinations);
    (0..combinations).find_map(|offset| {
        let candidate = candidate((start + offset) % combinations)?;
        (!existing.contains(&candidate)).then_some(candidate)
    })
}

fn candidate(index: usize) -> Option<String> {
    let adjective = ADJECTIVES[index / second_word_count()];
    let second = second_word(index % second_word_count());
    (adjective != second).then(|| format!("{adjective}-{second}"))
}

const fn second_word_count() -> usize {
    NOUNS.len() + ADJECTIVES.len()
}

fn second_word(index: usize) -> &'static str {
    if index < NOUNS.len() {
        NOUNS[index]
    } else {
        ADJECTIVES[index - NOUNS.len()]
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng as _;
    use rand::rngs::StdRng;

    use super::*;

    #[test]
    fn generated_names_use_two_catalog_words() {
        let generated = generate_with_rng(&mut StdRng::seed_from_u64(7), &BTreeSet::new())
            .unwrap_or_else(|| panic!("catalog contains available names"));
        let (adjective, second) = generated
            .split_once('-')
            .unwrap_or_else(|| panic!("name has two words"));

        assert!(ADJECTIVES.contains(&adjective));
        assert!(NOUNS.contains(&second) || ADJECTIVES.contains(&second));
        assert_ne!(adjective, second);
        assert!(crate::validate_profile_name(&generated).is_ok());
    }

    #[test]
    fn generated_names_skip_existing_profiles() {
        let mut first_rng = StdRng::seed_from_u64(11);
        let first = generate_with_rng(&mut first_rng, &BTreeSet::new())
            .unwrap_or_else(|| panic!("catalog contains available names"));
        let existing = BTreeSet::from([first.clone()]);
        let second = generate_with_rng(&mut StdRng::seed_from_u64(11), &existing)
            .unwrap_or_else(|| panic!("catalog contains another available name"));

        assert_ne!(first, second);
    }

    #[test]
    fn reports_exhaustion_only_after_every_name_is_used() {
        let existing = (0..ADJECTIVES.len() * second_word_count())
            .filter_map(candidate)
            .collect::<BTreeSet<_>>();

        assert_eq!(
            generate_with_rng(&mut StdRng::seed_from_u64(19), &existing),
            None
        );
    }
}
