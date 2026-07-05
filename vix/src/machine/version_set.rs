use std::cmp::Ordering;
use std::collections::BTreeSet;

use semver::{BuildMetadata, Comparator, Op, Prerelease, Version, VersionReq};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct VersionSet {
    releases: IntervalSet,
    prereleases: IntervalSet,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct IntervalSet {
    intervals: Vec<Interval>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Interval {
    start: Option<Version>,
    end: Option<Version>,
}

impl VersionSet {
    pub(super) fn universe() -> Self {
        Self {
            releases: IntervalSet::universe(),
            prereleases: IntervalSet::universe(),
        }
    }

    pub(super) fn from_req(text: &str) -> Result<Self, String> {
        let req = VersionReq::parse(text)
            .map_err(|err| format!("VersionSet::from_req({text:?}) parse error: {err}"))?;
        let mut set = Self::universe();
        let mut prerelease_bases = BTreeSet::new();
        for cmp in &req.comparators {
            set = set.intersect(&Self::from_comparator(cmp)?);
            if !cmp.pre.is_empty()
                && let (Some(minor), Some(patch)) = (cmp.minor, cmp.patch)
            {
                prerelease_bases.insert((cmp.major, minor, patch));
            }
        }
        if prerelease_bases.is_empty() {
            set.prereleases = IntervalSet::empty();
        } else {
            let admitted = prerelease_bases
                .into_iter()
                .map(|(major, minor, patch)| IntervalSet::prerelease_base(major, minor, patch))
                .fold(IntervalSet::empty(), |acc, next| acc.union(&next));
            set.prereleases = set.prereleases.intersect(&admitted);
        }
        Ok(set)
    }

    pub(super) fn parse_bytes(bytes: &[u8]) -> Result<Self, String> {
        let text = std::str::from_utf8(bytes).map_err(|err| err.to_string())?;
        let mut releases = IntervalSet::empty();
        let mut prereleases = IntervalSet::empty();
        for line in text.lines() {
            let Some((kind, rest)) = line.split_once(' ') else {
                return Err(format!("bad VersionSet line {line:?}"));
            };
            let Some((start, end)) = rest.split_once(' ') else {
                return Err(format!("bad VersionSet line {line:?}"));
            };
            let interval = Interval {
                start: decode_bound(start)?,
                end: decode_bound(end)?,
            };
            match kind {
                "R" => releases.intervals.push(interval),
                "P" => prereleases.intervals.push(interval),
                _ => return Err(format!("bad VersionSet interval kind {kind:?}")),
            }
        }
        releases.normalize();
        prereleases.normalize();
        Ok(Self {
            releases,
            prereleases,
        })
    }

    pub(super) fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = String::new();
        for interval in &self.releases.intervals {
            out.push_str("R ");
            out.push_str(&encode_bound(&interval.start));
            out.push(' ');
            out.push_str(&encode_bound(&interval.end));
            out.push('\n');
        }
        for interval in &self.prereleases.intervals {
            out.push_str("P ");
            out.push_str(&encode_bound(&interval.start));
            out.push(' ');
            out.push_str(&encode_bound(&interval.end));
            out.push('\n');
        }
        out.into_bytes()
    }

    pub(super) fn contains(&self, version: &Version) -> bool {
        if version.pre.is_empty() {
            self.releases.contains(version)
        } else {
            self.prereleases.contains(version)
        }
    }

    pub(super) fn union(&self, other: &Self) -> Self {
        Self {
            releases: self.releases.union(&other.releases),
            prereleases: self.prereleases.union(&other.prereleases),
        }
    }

    pub(super) fn intersect(&self, other: &Self) -> Self {
        Self {
            releases: self.releases.intersect(&other.releases),
            prereleases: self.prereleases.intersect(&other.prereleases),
        }
    }

    pub(super) fn complement(&self) -> Self {
        Self {
            releases: self.releases.complement(),
            prereleases: self.prereleases.complement(),
        }
    }

    pub(super) fn is_subset_of(&self, other: &Self) -> bool {
        self.releases.is_subset_of(&other.releases)
            && self.prereleases.is_subset_of(&other.prereleases)
    }

    pub(super) fn render(&self) -> String {
        String::from_utf8(self.canonical_bytes()).expect("VersionSet bytes are utf8")
    }

    fn from_comparator(cmp: &Comparator) -> Result<Self, String> {
        let interval = comparator_interval(cmp)?;
        Ok(Self {
            releases: IntervalSet::from_interval(interval.clone()),
            prereleases: IntervalSet::from_interval(interval),
        })
    }
}

impl IntervalSet {
    fn empty() -> Self {
        Self {
            intervals: Vec::new(),
        }
    }

    fn universe() -> Self {
        Self {
            intervals: vec![Interval {
                start: None,
                end: None,
            }],
        }
    }

    fn from_interval(interval: Interval) -> Self {
        let mut set = Self {
            intervals: vec![interval],
        };
        set.normalize();
        set
    }

    fn prerelease_base(major: u64, minor: u64, patch: u64) -> Self {
        Self::from_interval(Interval {
            start: Some(version_with_pre(major, minor, patch, "0")),
            end: Some(release(major, minor, patch)),
        })
    }

    fn contains(&self, version: &Version) -> bool {
        self.intervals
            .iter()
            .any(|interval| interval.contains(version))
    }

    fn union(&self, other: &Self) -> Self {
        let mut intervals = self.intervals.clone();
        intervals.extend(other.intervals.clone());
        let mut set = Self { intervals };
        set.normalize();
        set
    }

    fn intersect(&self, other: &Self) -> Self {
        let mut intervals = Vec::new();
        for left in &self.intervals {
            for right in &other.intervals {
                if let Some(interval) = left.intersect(right) {
                    intervals.push(interval);
                }
            }
        }
        let mut set = Self { intervals };
        set.normalize();
        set
    }

    fn complement(&self) -> Self {
        let mut out = Vec::new();
        let mut cursor = None;
        for interval in &self.intervals {
            if cursor_bound_before(&cursor, &interval.start) {
                out.push(Interval {
                    start: cursor.clone(),
                    end: interval.start.clone(),
                });
            }
            cursor = interval.end.clone();
        }
        if cursor.is_some() {
            out.push(Interval {
                start: cursor,
                end: None,
            });
        }
        let mut set = Self { intervals: out };
        set.normalize();
        set
    }

    fn is_subset_of(&self, other: &Self) -> bool {
        self.intersect(&other.complement()).intervals.is_empty()
    }

    fn normalize(&mut self) {
        self.intervals.retain(|interval| !interval.is_empty());
        self.intervals.sort_by(compare_interval_start);
        let mut normalized: Vec<Interval> = Vec::new();
        for interval in self.intervals.drain(..) {
            if let Some(last) = normalized.last_mut()
                && intervals_touch_or_overlap(last, &interval)
            {
                last.end = max_end(last.end.clone(), interval.end);
                continue;
            }
            normalized.push(interval);
        }
        self.intervals = normalized;
    }
}

impl Interval {
    fn contains(&self, version: &Version) -> bool {
        let after_start = self
            .start
            .as_ref()
            .is_none_or(|start| version.cmp(start) != Ordering::Less);
        let before_end = self
            .end
            .as_ref()
            .is_none_or(|end| version.cmp(end) == Ordering::Less);
        after_start && before_end
    }

    fn intersect(&self, other: &Self) -> Option<Self> {
        let interval = Self {
            start: max_start(self.start.clone(), other.start.clone()),
            end: min_end(self.end.clone(), other.end.clone()),
        };
        (!interval.is_empty()).then_some(interval)
    }

    fn is_empty(&self) -> bool {
        match (&self.start, &self.end) {
            (Some(start), Some(end)) => start >= end,
            _ => false,
        }
    }
}

fn comparator_interval(cmp: &Comparator) -> Result<Interval, String> {
    Ok(match cmp.op {
        Op::Exact | Op::Wildcard => exact_interval(cmp)?,
        Op::Greater => Interval {
            start: Some(greater_lower(cmp)?),
            end: None,
        },
        Op::GreaterEq => Interval {
            start: Some(comparator_base(cmp)?),
            end: None,
        },
        Op::Less => Interval {
            start: None,
            end: Some(comparator_base(cmp)?),
        },
        Op::LessEq => Interval {
            start: None,
            end: Some(less_eq_upper(cmp)?),
        },
        Op::Tilde => tilde_interval(cmp)?,
        Op::Caret => caret_interval(cmp)?,
        _ => return Err(format!("unsupported semver comparator op {:?}", cmp.op)),
    })
}

fn exact_interval(cmp: &Comparator) -> Result<Interval, String> {
    let start = release(cmp.major, cmp.minor.unwrap_or(0), cmp.patch.unwrap_or(0));
    let end = match (cmp.minor, cmp.patch, cmp.pre.is_empty()) {
        (None, _, _) => release(cmp.major + 1, 0, 0),
        (Some(minor), None, _) => release(cmp.major, minor + 1, 0),
        (Some(minor), Some(patch), true) => version_with_pre(cmp.major, minor, patch + 1, "0"),
        (Some(minor), Some(patch), false) => {
            prerelease_successor(cmp.major, minor, patch, &cmp.pre)?
        }
    };
    let start = if cmp.patch.is_some() && !cmp.pre.is_empty() {
        comparator_base(cmp)?
    } else {
        start
    };
    Ok(Interval {
        start: Some(start),
        end: Some(end),
    })
}

fn tilde_interval(cmp: &Comparator) -> Result<Interval, String> {
    let minor = cmp.minor.unwrap_or(0);
    let patch = cmp.patch.unwrap_or(0);
    let end = if cmp.minor.is_some() {
        release(cmp.major, minor + 1, 0)
    } else {
        release(cmp.major + 1, 0, 0)
    };
    Ok(Interval {
        start: Some(if cmp.patch.is_some() && !cmp.pre.is_empty() {
            comparator_base(cmp)?
        } else {
            release(cmp.major, minor, patch)
        }),
        end: Some(end),
    })
}

fn caret_interval(cmp: &Comparator) -> Result<Interval, String> {
    let minor = cmp.minor.unwrap_or(0);
    let patch = cmp.patch.unwrap_or(0);
    let start = if cmp.patch.is_some() && !cmp.pre.is_empty() {
        comparator_base(cmp)?
    } else {
        release(cmp.major, minor, patch)
    };
    let end = if cmp.major > 0 || cmp.minor.is_none() {
        release(cmp.major + 1, 0, 0)
    } else if minor > 0 || cmp.patch.is_none() {
        release(0, minor + 1, 0)
    } else {
        version_with_pre(0, 0, patch + 1, "0")
    };
    Ok(Interval {
        start: Some(start),
        end: Some(end),
    })
}

fn greater_lower(cmp: &Comparator) -> Result<Version, String> {
    Ok(match (cmp.minor, cmp.patch, cmp.pre.is_empty()) {
        (None, _, _) => release(cmp.major + 1, 0, 0),
        (Some(minor), None, _) => release(cmp.major, minor + 1, 0),
        (Some(minor), Some(patch), true) => version_with_pre(cmp.major, minor, patch + 1, "0"),
        (Some(minor), Some(patch), false) => {
            prerelease_successor(cmp.major, minor, patch, &cmp.pre)?
        }
    })
}

fn less_eq_upper(cmp: &Comparator) -> Result<Version, String> {
    Ok(match (cmp.minor, cmp.patch, cmp.pre.is_empty()) {
        (None, _, _) => release(cmp.major + 1, 0, 0),
        (Some(minor), None, _) => release(cmp.major, minor + 1, 0),
        (Some(minor), Some(patch), true) => version_with_pre(cmp.major, minor, patch + 1, "0"),
        (Some(minor), Some(patch), false) => {
            prerelease_successor(cmp.major, minor, patch, &cmp.pre)?
        }
    })
}

fn comparator_base(cmp: &Comparator) -> Result<Version, String> {
    let minor = cmp.minor.unwrap_or(0);
    let patch = cmp.patch.unwrap_or(0);
    if cmp.pre.is_empty() {
        Ok(release(cmp.major, minor, patch))
    } else {
        Ok(Version {
            major: cmp.major,
            minor,
            patch,
            pre: cmp.pre.clone(),
            build: BuildMetadata::EMPTY,
        })
    }
}

fn prerelease_successor(
    major: u64,
    minor: u64,
    patch: u64,
    pre: &Prerelease,
) -> Result<Version, String> {
    version_with_pre_result(major, minor, patch, &format!("{pre}.0"))
}

fn release(major: u64, minor: u64, patch: u64) -> Version {
    Version {
        major,
        minor,
        patch,
        pre: Prerelease::EMPTY,
        build: BuildMetadata::EMPTY,
    }
}

fn version_with_pre(major: u64, minor: u64, patch: u64, pre: &str) -> Version {
    version_with_pre_result(major, minor, patch, pre).expect("static prerelease parses")
}

fn version_with_pre_result(
    major: u64,
    minor: u64,
    patch: u64,
    pre: &str,
) -> Result<Version, String> {
    Ok(Version {
        major,
        minor,
        patch,
        pre: Prerelease::new(pre)
            .map_err(|err| format!("VersionSet prerelease bound {pre:?}: {err}"))?,
        build: BuildMetadata::EMPTY,
    })
}

fn encode_bound(bound: &Option<Version>) -> String {
    bound.as_ref().map_or("*".to_string(), Version::to_string)
}

fn decode_bound(text: &str) -> Result<Option<Version>, String> {
    if text == "*" {
        Ok(None)
    } else {
        Version::parse(text)
            .map(Some)
            .map_err(|err| format!("bad VersionSet bound {text:?}: {err}"))
    }
}

fn compare_interval_start(left: &Interval, right: &Interval) -> Ordering {
    match (&left.start, &right.start) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(left), Some(right)) => left.cmp(right),
    }
}

fn intervals_touch_or_overlap(left: &Interval, right: &Interval) -> bool {
    match (&left.end, &right.start) {
        (None, _) => true,
        (_, None) => true,
        (Some(end), Some(start)) => start <= end,
    }
}

fn cursor_bound_before(left: &Option<Version>, right: &Option<Version>) -> bool {
    match (left, right) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(left), Some(right)) => left < right,
    }
}

fn max_start(left: Option<Version>, right: Option<Version>) -> Option<Version> {
    match (left, right) {
        (None, other) | (other, None) => other,
        (Some(left), Some(right)) => Some(left.max(right)),
    }
}

fn min_end(left: Option<Version>, right: Option<Version>) -> Option<Version> {
    match (left, right) {
        (None, other) | (other, None) => other,
        (Some(left), Some(right)) => Some(left.min(right)),
    }
}

fn max_end(left: Option<Version>, right: Option<Version>) -> Option<Version> {
    match (left, right) {
        (None, _) | (_, None) => None,
        (Some(left), Some(right)) => Some(left.max(right)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn cargo_prerelease_matching_is_pinned() {
        let req = VersionReq::parse("^1.2.3-alpha.1").unwrap();
        let set = VersionSet::from_req("^1.2.3-alpha.1").unwrap();
        for version in [
            "1.2.3-alpha.0",
            "1.2.3-alpha.1",
            "1.2.3-beta.1",
            "1.2.3",
            "1.2.4-alpha.1",
            "1.2.4",
            "2.0.0-alpha.1",
            "2.0.0",
        ] {
            let version = v(version);
            assert_eq!(set.contains(&version), req.matches(&version), "{version}");
        }
    }

    #[test]
    fn caret_zero_ranges_are_pinned() {
        let req = VersionReq::parse("^0.2.3").unwrap();
        let set = VersionSet::from_req("^0.2.3").unwrap();
        for version in ["0.2.2", "0.2.3", "0.2.9", "0.3.0-alpha.1", "0.3.0"] {
            let version = v(version);
            assert_eq!(set.contains(&version), req.matches(&version), "{version}");
        }

        let req = VersionReq::parse("^0.0.3").unwrap();
        let set = VersionSet::from_req("^0.0.3").unwrap();
        for version in ["0.0.3-alpha.1", "0.0.3", "0.0.4-alpha.1", "0.0.4"] {
            let version = v(version);
            assert_eq!(set.contains(&version), req.matches(&version), "{version}");
        }
    }

    #[test]
    fn de_morgan_laws_hold_for_interval_sets() {
        let a = VersionSet::from_req(">=1.2.0, <2.0.0").unwrap();
        let b = VersionSet::from_req("^1.5.0").unwrap();
        assert_eq!(
            a.intersect(&b).complement(),
            a.complement().union(&b.complement())
        );
        assert_eq!(
            a.union(&b).complement(),
            a.complement().intersect(&b.complement())
        );
    }
}
