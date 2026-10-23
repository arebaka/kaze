use std::sync::Arc;

use icu_collator::options::CollatorOptions;
use icu_collator::preferences::CollationNumericOrdering;
use icu_collator::{Collator, CollatorBorrowed, CollatorPreferences};
use icu_locale_core::Locale;

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct CollationKey(Vec<u8>);

#[derive(Debug, Clone)]
pub struct Collation {
    collator: Option<Arc<CollatorBorrowed<'static>>>,
}

impl Collation {
    pub fn new(language: &str) -> Self {
        let locale = Locale::try_from_str(language).unwrap_or(Locale::UNKNOWN);
        let mut preferences = CollatorPreferences::from(&locale);
        preferences.numeric_ordering = Some(CollationNumericOrdering::True);
        let collator = Collator::try_new(preferences, CollatorOptions::default());
        Self {
            collator: collator.ok().map(Arc::new),
        }
    }

    pub fn key(&self, name: &str) -> CollationKey {
        let mut key = Vec::new();
        match &self.collator {
            Some(collator) => {
                let Ok(()) = collator.write_sort_key_to(name, &mut key);
            }
            None => key.extend_from_slice(name.as_bytes()),
        }
        CollationKey(key)
    }
}

impl Default for Collation {
    fn default() -> Self {
        Self::new("und")
    }
}
