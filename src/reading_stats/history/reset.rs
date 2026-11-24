use super::*;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub(super) struct Baseline {
    elapsed: u64,
    generation: u64,
}

pub(crate) const SHORT_READING_MINUTES: [u16; 5] = [1, 5, 15, 30, 60];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    Book(u64),
    ShortBooks { minutes: u16, keep: Option<u64> },
}

#[derive(Clone, Copy, Default)]
pub(crate) struct BookStatistics {
    pub seconds: u64,
    pub last_visit: Option<i64>,
}

fn visit_in_period(visit: &Visit, book: u64, from: i64, until: i64) -> bool {
    visit.book == book
        && visit
            .start
            .is_some_and(|start| (from..until).contains(&start))
}

impl History {
    pub(crate) fn count_period_visits(&self, book: u64, from: i64, until: i64) -> usize {
        self.data
            .visits
            .iter()
            .filter(|visit| visit_in_period(visit, book, from, until))
            .count()
    }

    pub(crate) fn reset_period(
        &mut self,
        book: u64,
        from: i64,
        until: i64,
        now: Instant,
        wall: Option<i64>,
    ) -> io::Result<usize> {
        if self.read_only || from >= until {
            return Err(invalid());
        }
        let before = self.data.visits.clone();
        let active = self.active.clone();
        let dirty = self.dirty;
        self.accrue(now, wall);
        let count = self.count_period_visits(book, from, until);
        if count == 0 {
            return Ok(0);
        }
        self.data
            .visits
            .retain(|visit| !visit_in_period(visit, book, from, until));
        if let Err(error) = self.persist() {
            self.data.visits = before;
            self.active = active;
            self.dirty = dirty;
            return Err(error);
        }
        // The current visit keeps its existing credit and limit. Future
        // elapsed time can be recorded without restoring deleted intervals.
        self.dirty = false;
        Ok(count)
    }

    pub(crate) fn reset_books(&self) -> BTreeMap<u64, BookStatistics> {
        let mut books: BTreeMap<_, _> = self
            .data
            .prior
            .iter()
            .map(|(&id, &seconds)| {
                (
                    id,
                    BookStatistics {
                        seconds,
                        last_visit: None,
                    },
                )
            })
            .collect();
        for visit in &self.data.visits {
            let totals = books.entry(visit.book).or_default();
            totals.seconds = totals.seconds.saturating_add(u64::from(visit.seconds));
            totals.last_visit = totals.last_visit.max(visit.start);
        }
        books
    }

    pub(crate) fn reset_selection(&self, scope: Scope) -> BTreeSet<u64> {
        match scope {
            Scope::Book(id) => BTreeSet::from([id]),
            Scope::ShortBooks { minutes, keep } => {
                let cutoff = u64::from(minutes) * 60;
                self.reset_books()
                    .into_iter()
                    .filter(|&(id, value)| Some(id) != keep && value.seconds < cutoff)
                    .map(|(id, _)| id)
                    .collect()
            }
        }
    }

    pub(crate) fn elapsed_since_reset(&self, book: u64, accumulated: u64) -> u64 {
        accumulated.saturating_sub(self.data.resets.get(&book).map_or(0, |r| r.elapsed))
    }

    pub(crate) fn pace_generation(&self, book: u64) -> u64 {
        self.data.resets.get(&book).map_or(0, |r| r.generation)
    }

    pub(crate) fn reset(
        &mut self,
        scope: Scope,
        accumulated: &BTreeMap<u64, u64>,
        now: Instant,
        wall: Option<i64>,
    ) -> io::Result<BTreeSet<u64>> {
        if self.read_only {
            return Err(invalid());
        }
        let before = self.data.clone();
        let active = self.active.clone();
        let dirty = self.dirty;
        self.accrue(now, wall);
        let selection = self.reset_selection(scope);
        if selection.is_empty() {
            return Ok(selection);
        }
        let result = (|| {
            for &id in &selection {
                let baseline = self.data.resets.entry(id).or_default();
                baseline.elapsed = accumulated.get(&id).copied().unwrap_or_default();
                baseline.generation = baseline.generation.checked_add(1).ok_or_else(invalid)?;
                self.data.prior.remove(&id);
                self.data.titles.remove(&id);
            }
            if self.data.resets.len() > MAX_BOOKS {
                return Err(invalid());
            }
            self.data
                .visits
                .retain(|visit| !selection.contains(&visit.book));
            self.persist()
        })();
        if let Err(error) = result {
            self.data = before;
            self.active = active;
            self.dirty = dirty;
            return Err(error);
        }
        if self
            .active
            .as_ref()
            .is_some_and(|visit| selection.contains(&visit.book))
        {
            self.active = None;
        }
        self.dirty = false;
        Ok(selection)
    }
}
