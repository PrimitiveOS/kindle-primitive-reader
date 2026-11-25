//! Reader-authored starts of hidden and regular reading flows.
use super::{ReadingPosition, invalid_state, same_position, valid_position};
use serde::{Deserialize, Serialize};
use std::io;

const MAX_POINTS: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct FlowPoint {
    pub position: ReadingPosition,
    pub hidden: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct HiddenFlows {
    pub enabled: bool,
    pub editing: bool,
    points: Vec<FlowPoint>,
}

impl Default for HiddenFlows {
    fn default() -> Self {
        Self {
            enabled: false,
            editing: true,
            points: Vec::new(),
        }
    }
}

fn source_order(position: &ReadingPosition) -> ((usize, usize, usize), (u8, usize, [u8; 32], u8)) {
    super::raw_positions::flow_order(position)
}

impl HiddenFlows {
    pub fn is_default(&self) -> bool {
        !self.enabled && self.editing && self.points.is_empty()
    }

    pub fn points(&self) -> &[FlowPoint] {
        &self.points
    }

    #[cfg(feature = "kindle")]
    pub(super) fn points_mut_for_odt_migration(&mut self) -> &mut [FlowPoint] {
        &mut self.points
    }

    #[cfg(feature = "kindle")]
    pub(super) fn sort_after_odt_migration(&mut self) -> bool {
        if self.points.iter().enumerate().any(|(index, point)| {
            self.points[..index]
                .iter()
                .any(|other| same_position(&point.position, &other.position))
        }) {
            return false;
        }
        self.points
            .sort_by_key(|point| source_order(&point.position));
        true
    }

    pub fn insert(&mut self, point: FlowPoint) -> io::Result<bool> {
        if !valid_position(&point.position) {
            return Err(invalid_state(
                "This flow boundary has no valid source location",
            ));
        }
        if self
            .points
            .iter()
            .any(|p| same_position(&p.position, &point.position))
        {
            return Ok(false);
        }
        if self.points.len() == MAX_POINTS {
            return Err(invalid_state("This book already has 512 flow boundaries"));
        }
        let index = self
            .points
            .partition_point(|p| source_order(&p.position) <= source_order(&point.position));
        self.points.insert(index, point);
        Ok(true)
    }

    pub fn toggle(&mut self, position: ReadingPosition, existing: Option<usize>) -> io::Result<()> {
        if let Some(index) = existing {
            if index >= self.points.len() {
                return Err(invalid_state("That boundary is unavailable"));
            }
            self.points.remove(index);
            return Ok(());
        }
        let index = self
            .points
            .partition_point(|p| source_order(&p.position) <= source_order(&position));
        let hidden = index == 0 || !self.points[index - 1].hidden;
        self.insert(FlowPoint { position, hidden })?;
        Ok(())
    }

    pub fn remove(&mut self, index: usize) -> io::Result<()> {
        if index >= self.points.len() {
            return Err(invalid_state("That boundary is unavailable"));
        }
        self.points.remove(index);
        Ok(())
    }

    pub fn clear(&mut self) {
        self.points.clear();
    }

    pub fn sanitize(&mut self) {
        let mut points = std::mem::take(&mut self.points);
        points.truncate(MAX_POINTS);
        for point in points {
            let _ = self.insert(point);
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ResolvedPoint {
    pub page: usize,
    pub hidden: bool,
    pub index: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PageFlows {
    count: usize,
    // Starts are sorted; id 0 is the regular flow, other ids identify source markers.
    starts: Vec<(usize, usize)>,
    pub inactive: Vec<usize>,
}

impl PageFlows {
    pub fn has_hidden(&self) -> bool {
        self.starts.iter().any(|&(_, flow)| flow != 0)
    }

    pub fn new(count: usize, inherited: usize, mut points: Vec<ResolvedPoint>) -> Self {
        points.sort_by_key(|point| (point.page, point.index));
        let mut result = Self {
            count,
            starts: vec![(0, inherited)],
            inactive: Vec::new(),
        };
        let mut current = inherited;
        for point in points.into_iter().filter(|point| point.page < count) {
            if point.hidden == (current != 0) {
                result.inactive.push(point.index);
                continue;
            }
            current = if point.hidden { point.index + 1 } else { 0 };
            if result
                .starts
                .last()
                .is_some_and(|&(page, _)| page == point.page)
            {
                result.starts.pop();
            }
            result.starts.push((point.page, current));
        }
        result
    }

    pub fn hidden_ids(&self) -> impl Iterator<Item = usize> + '_ {
        self.starts
            .iter()
            .enumerate()
            .filter_map(|(index, &(start, flow))| {
                let end = self
                    .starts
                    .get(index + 1)
                    .map_or(self.count, |&(end, _)| end);
                (flow != 0 && start < end).then_some(flow)
            })
    }

    pub fn flow_ordinal(&self, flow: usize) -> Option<usize> {
        self.hidden_ids()
            .position(|id| id == flow)
            .map(|index| index + 1)
    }

    pub fn flow(&self, page: usize) -> usize {
        let i = self
            .starts
            .partition_point(|&(start, _)| start <= page)
            .saturating_sub(1);
        self.starts.get(i).map_or(0, |&(_, flow)| flow)
    }

    pub fn allowed_page(
        &self,
        start: usize,
        direction: isize,
        current_flow: usize,
    ) -> Option<usize> {
        let mut page = start;
        loop {
            if page >= self.count {
                return None;
            }
            let i = self
                .starts
                .partition_point(|&(first, _)| first <= page)
                .saturating_sub(1);
            let &(first, flow) = self.starts.get(i)?;
            if flow == 0 || flow == current_flow {
                return Some(page);
            }
            page = if direction > 0 {
                self.starts
                    .get(i + 1)
                    .map_or(self.count, |&(first, _)| first)
            } else {
                first.checked_sub(1)?
            };
        }
    }

    pub fn next(&self, page: usize, direction: isize) -> Option<usize> {
        self.allowed_page(
            page.checked_add_signed(direction)?,
            direction,
            self.flow(page),
        )
    }

    pub(crate) fn relative_page(&self, page: usize, delta: isize) -> Option<usize> {
        if page >= self.count {
            return None;
        }
        let ranges = self.allowed_ranges(self.flow(page));
        let mut offset = 0;
        let mut current = None;
        for range in &ranges {
            if range.contains(&page) {
                current = Some(offset + page - range.start);
            }
            offset += range.len();
        }
        let target = current?
            .saturating_add_signed(delta)
            .min(offset.checked_sub(1)?);
        let mut remainder = target;
        for range in ranges {
            if remainder < range.len() {
                return Some(range.start + remainder);
            }
            remainder -= range.len();
        }
        None
    }

    pub fn count_in(&self, flow: usize, range: std::ops::Range<usize>) -> usize {
        self.starts
            .iter()
            .enumerate()
            .filter(|&(_, &(_, id))| id == flow)
            .map(|(i, &(start, _))| {
                let end = self.starts.get(i + 1).map_or(self.count, |&(end, _)| end);
                end.min(range.end).saturating_sub(start.max(range.start))
            })
            .sum()
    }

    pub fn nth(&self, flow: usize, mut index: usize) -> Option<usize> {
        for (i, &(start, id)) in self.starts.iter().enumerate() {
            if id != flow {
                continue;
            }
            let end = self.starts.get(i + 1).map_or(self.count, |&(end, _)| end);
            if index < end - start {
                return Some(start + index);
            }
            index -= end - start;
        }
        None
    }

    pub fn allowed_ranges(&self, flow: usize) -> Vec<std::ops::Range<usize>> {
        let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
        for (index, &(start, id)) in self.starts.iter().enumerate() {
            if id != 0 && id != flow {
                continue;
            }
            let end = self
                .starts
                .get(index + 1)
                .map_or(self.count, |&(end, _)| end);
            if let Some(previous) = ranges.last_mut().filter(|previous| previous.end == start) {
                previous.end = end;
            } else {
                ranges.push(start..end);
            }
        }
        ranges
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::BookState;

    #[test]
    fn flow_boundaries_preserve_order_inactive_points_and_reference_navigation() {
        let mut data = HiddenFlows {
            enabled: true,
            ..HiddenFlows::default()
        };
        let position = |page| ReadingPosition {
            chapter: 0,
            page,
            anchor: None,
        };
        for page in [2, 6, 9] {
            data.toggle(position(page), None).unwrap();
        }
        assert_eq!(
            data.points
                .iter()
                .map(|point| point.hidden)
                .collect::<Vec<_>>(),
            [true, false, true]
        );
        data.toggle(position(3), None).unwrap();
        assert_eq!(
            data.points
                .iter()
                .map(|point| point.hidden)
                .collect::<Vec<_>>(),
            [true, false, false, true]
        );
        let resolved: Vec<_> = data
            .points
            .iter()
            .enumerate()
            .map(|(index, point)| ResolvedPoint {
                page: point.position.page,
                hidden: point.hidden,
                index,
            })
            .collect();
        let map = PageFlows::new(12, 0, resolved);
        assert_eq!(map.inactive, [2]);
        assert_eq!(map.next(1, 1), Some(3));
        assert_eq!(map.next(3, -1), Some(1));
        assert_eq!(map.next(2, 1), Some(3));
        assert_eq!(map.next(8, 1), None);
        assert_eq!(map.next(9, 1), Some(10));
        assert_eq!(map.count_in(0, 0..12), 8);
        assert_eq!(map.hidden_ids().collect::<Vec<_>>(), [1, 4]);
        assert_eq!(map.flow_ordinal(4), Some(2));
        let collapsed = PageFlows::new(
            12,
            0,
            vec![
                ResolvedPoint {
                    page: 2,
                    hidden: true,
                    index: 0,
                },
                ResolvedPoint {
                    page: 2,
                    hidden: false,
                    index: 1,
                },
                ResolvedPoint {
                    page: 4,
                    hidden: true,
                    index: 2,
                },
                ResolvedPoint {
                    page: 7,
                    hidden: false,
                    index: 3,
                },
            ],
        );
        assert_eq!(collapsed.hidden_ids().collect::<Vec<_>>(), [3]);
        assert_eq!(collapsed.flow_ordinal(3), Some(1));
        let text = serde_json::to_vec(&data).unwrap();
        let mut restored: HiddenFlows = serde_json::from_slice(&text).unwrap();
        restored.sanitize();
        assert_eq!(restored.points.len(), 4);
        assert!(restored.enabled && restored.editing);
    }

    #[test]
    fn original_flow_algorithms_match_the_pinned_reference_traces() {
        let data: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/hidden-flows-reference.json"
        ))
        .unwrap();
        for (case_index, case) in data["cases"].as_array().unwrap().iter().enumerate() {
            let mut state = HiddenFlows::default();
            for page in case["toggles"].as_array().unwrap() {
                let page = page.as_u64().unwrap() as usize;
                let existing = state
                    .points()
                    .iter()
                    .position(|point| point.position.page == page);
                state
                    .toggle(
                        ReadingPosition {
                            chapter: 0,
                            page,
                            anchor: None,
                        },
                        existing,
                    )
                    .unwrap();
            }
            assert_eq!(
                state
                    .points()
                    .iter()
                    .map(|point| (point.position.page, point.hidden))
                    .collect::<Vec<_>>(),
                case["points"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|point| (
                        point["page"].as_u64().unwrap() as usize,
                        point["hidden"].as_bool().unwrap()
                    ))
                    .collect::<Vec<_>>()
            );
            let count = case["count"].as_u64().unwrap() as usize;
            let points = state
                .points()
                .iter()
                .enumerate()
                .map(|(index, point)| ResolvedPoint {
                    index,
                    page: point.position.page,
                    hidden: point.hidden,
                })
                .collect();
            let flows = PageFlows::new(count, 0, points);
            assert_eq!(
                flows.count_in(0, 0..count),
                case["regular_pages"].as_u64().unwrap() as usize,
                "case {case_index}"
            );
            assert_eq!(
                flows.inactive,
                case["inactive"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|n| n.as_u64().unwrap() as usize)
                    .collect::<Vec<_>>()
            );
            for page in 0..count {
                assert_eq!(
                    flows.flow(page) != 0,
                    case["hidden"][page].as_bool().unwrap(),
                    "case {case_index} page {page}"
                );
                for (name, direction) in [("next", 1), ("previous", -1)] {
                    let mut expected = case[name][page].as_i64().unwrap();
                    if name == "next"
                        && let Some(corrected) =
                            case["next_reference_defects"][page.to_string()].as_i64()
                    {
                        assert_eq!(expected, -1);
                        assert_eq!(corrected, count as i64 - 1);
                        assert!(case["reported_last_regular"].as_i64().unwrap() < corrected);
                        expected = corrected;
                    }
                    assert_eq!(
                        flows.next(page, direction),
                        (expected >= 0).then_some(expected as usize),
                        "case {case_index} page {page} {name}"
                    );
                }
            }
        }
    }

    #[test]
    fn stored_flow_boundaries_validate_source_paths_and_bound_untrusted_backups() {
        let mut data = HiddenFlows::default();
        let mut bad: ReadingPosition = serde_json::from_str(r#"{"chapter":0,"page":0,"anchor":{"book":0,"chapter":"","revision":0,"block":0,"offset":0}}"#).unwrap();
        assert!(
            data.insert(FlowPoint {
                position: bad.clone(),
                hidden: true
            })
            .is_err()
        );
        bad.anchor.as_mut().unwrap().chapter = "chapter\n.xhtml".into();
        assert!(
            data.insert(FlowPoint {
                position: bad,
                hidden: true
            })
            .is_err()
        );
        for page in 0..MAX_POINTS {
            data.insert(FlowPoint {
                position: ReadingPosition {
                    chapter: 0,
                    page,
                    anchor: None,
                },
                hidden: true,
            })
            .unwrap();
        }
        assert!(!data.insert(data.points()[0].clone()).unwrap());
        assert!(
            data.insert(FlowPoint {
                position: ReadingPosition {
                    chapter: 0,
                    page: MAX_POINTS,
                    anchor: None
                },
                hidden: false
            })
            .is_err()
        );
        assert_eq!(data.points().len(), MAX_POINTS);
        assert!(data.remove(MAX_POINTS).is_err());
        let raw = serde_json::json!({"enabled":true,"points":(0..MAX_POINTS + 80).map(|page|
            serde_json::json!({"position":{"chapter":0,"page":page},"hidden":true})).collect::<Vec<_>>()});
        let mut restored: HiddenFlows = serde_json::from_value(raw).unwrap();
        restored.sanitize();
        assert_eq!(restored.points().len(), MAX_POINTS);
    }

    #[test]
    fn source_flow_boundaries_merge_and_restore_once_with_existing_book_overrides() {
        let initial: BookState =
            serde_json::from_str(r#"{"id":4,"path":"book.pdf","title":"Field report"}"#).unwrap();
        let mut source = initial.clone();
        source.hidden_flows.enabled = true;
        source
            .hidden_flows
            .insert(FlowPoint {
                position: ReadingPosition {
                    chapter: 0,
                    page: 1,
                    anchor: None,
                },
                hidden: true,
            })
            .unwrap();
        source
            .hidden_flows
            .insert(FlowPoint {
                position: ReadingPosition {
                    chapter: 0,
                    page: 4,
                    anchor: None,
                },
                hidden: false,
            })
            .unwrap();
        let (mut restored, summary) = crate::library::merge_states(&[], &[source.clone()]);
        assert_eq!(summary.custom_flow_points_added, 2);
        assert!(restored[0].hidden_flows.enabled);
        assert_eq!(restored[0].hidden_flows.points().len(), 2);
        restored[0].hidden_flows.enabled = false;
        let (merged, summary) = crate::library::merge_states(&restored, &[source]);
        assert_eq!(summary.custom_flow_points_added, 0);
        assert_eq!(merged[0].hidden_flows.points().len(), 2);
        assert!(!merged[0].hidden_flows.enabled);
    }
}
