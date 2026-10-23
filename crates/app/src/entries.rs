use std::cell::RefCell;

use slint::{Model, ModelNotify, ModelTracker};

use crate::Entry;

#[derive(Default)]
pub(crate) struct EntryList {
    rows: RefCell<Vec<Entry>>,
    notify: ModelNotify,
}

impl EntryList {
    pub(crate) fn insert(&self, positions: &[usize], entries: Vec<Entry>) {
        let mut arrived: Vec<(usize, Entry)> = positions.iter().copied().zip(entries).collect();
        arrived.sort_by_key(|(position, _)| *position);
        let mut inserted = Vec::new();
        {
            let Ok(mut rows) = self.rows.try_borrow_mut() else {
                return;
            };
            let mut previous = std::mem::take(&mut *rows).into_iter();
            for (position, entry) in arrived {
                while rows.len() < position
                    && let Some(existing) = previous.next()
                {
                    rows.push(existing);
                }
                inserted.push(rows.len());
                rows.push(entry);
            }
            rows.extend(previous);
        }
        for position in inserted {
            self.notify.row_added(position, 1);
        }
    }

    pub(crate) fn reorder(&self, previous_indices: &[usize]) {
        {
            let Ok(mut rows) = self.rows.try_borrow_mut() else {
                return;
            };
            let previous = std::mem::take(&mut *rows);
            let mut previous: Vec<Option<Entry>> = previous.into_iter().map(Some).collect();
            for index in previous_indices {
                if let Some(entry) = previous.get_mut(*index).and_then(Option::take) {
                    rows.push(entry);
                }
            }
            rows.extend(previous.into_iter().flatten());
        }
        self.notify.reset();
    }
}

impl Model for EntryList {
    type Data = Entry;

    fn row_count(&self) -> usize {
        self.rows.try_borrow().map_or(0, |rows| rows.len())
    }

    fn row_data(&self, row: usize) -> Option<Entry> {
        self.rows.try_borrow().ok()?.get(row).cloned()
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}
