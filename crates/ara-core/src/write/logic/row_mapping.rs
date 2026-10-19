//! Validation of caller-written citer rows for a claim merge or split
//! (plan 19 C1). A row's `after` must equal its exact `before` except that
//! each citation of the primary (merge source) is replaced by an allowed
//! destination. Only list-typed values may fan one citation out into several
//! destinations; any other content change needs its own audited revision.

/// A field value as literal text and typed claim citations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Part {
    Lit(String),
    Claim { text: String, id: String },
}

/// Why a row's mapping is not a pure replacement of primary citations.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Mismatch {
    /// A replacement names a claim that is not an allowed destination.
    Destination(String),
    /// Other bytes changed, or a scalar citation fanned out.
    Content,
}

/// Separators that join list items in a fan-out.
const LIST_SEPARATORS: [&str; 4] = [", ", ",", "\", \"", "\",\""];

/// Check `after` against `before`: every primary citation becomes one
/// destination (several only in a list), every other part is unchanged.
pub(super) fn check(
    before: &[Part],
    after: &[Part],
    primary: &str,
    destinations: &std::collections::BTreeSet<String>,
    list: bool,
) -> Result<(), Mismatch> {
    let mut wrong = None;
    if align(before, after, primary, destinations, list, &mut wrong) {
        Ok(())
    } else {
        Err(wrong.map_or(Mismatch::Content, Mismatch::Destination))
    }
}

fn align(
    before: &[Part],
    after: &[Part],
    primary: &str,
    destinations: &std::collections::BTreeSet<String>,
    list: bool,
    wrong: &mut Option<String>,
) -> bool {
    match (before.first(), after.first()) {
        (None, None) => true,
        (Some(Part::Lit(left)), Some(Part::Lit(right))) => {
            left == right
                && align(
                    &before[1..],
                    &after[1..],
                    primary,
                    destinations,
                    list,
                    wrong,
                )
        }
        (
            Some(Part::Claim { text, id }),
            Some(Part::Claim {
                text: new,
                id: target,
            }),
        ) => {
            if id != primary {
                return text == new
                    && align(
                        &before[1..],
                        &after[1..],
                        primary,
                        destinations,
                        list,
                        wrong,
                    );
            }
            if !destinations.contains(target) {
                wrong.get_or_insert_with(|| target.clone());
                return false;
            }
            if align(
                &before[1..],
                &after[1..],
                primary,
                destinations,
                list,
                wrong,
            ) {
                return true;
            }
            if !list {
                return false;
            }
            let mut next = 1;
            while let (Some(Part::Lit(separator)), Some(Part::Claim { id, .. })) =
                (after.get(next), after.get(next + 1))
            {
                if !LIST_SEPARATORS.contains(&separator.as_str()) {
                    break;
                }
                if !destinations.contains(id) {
                    wrong.get_or_insert_with(|| id.clone());
                    break;
                }
                next += 2;
                if align(
                    &before[1..],
                    &after[next..],
                    primary,
                    destinations,
                    list,
                    wrong,
                ) {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(spec: &[&str]) -> Vec<Part> {
        spec.iter()
            .map(|item| match item.strip_prefix('@') {
                Some(id) => Part::Claim {
                    text: id.into(),
                    id: id.into(),
                },
                None => Part::Lit((*item).into()),
            })
            .collect()
    }

    #[test]
    fn only_primary_citations_change_and_only_lists_fan_out() {
        let destinations = ["C02".to_owned(), "C08".to_owned()].into();
        let before = parts(&["[", "@C02", ", ", "@C03", "]"]);
        let fan = parts(&["[", "@C02", ", ", "@C08", ", ", "@C03", "]"]);
        assert_eq!(check(&before, &fan, "C02", &destinations, true), Ok(()));
        assert_eq!(
            check(&before, &fan, "C02", &destinations, false),
            Err(Mismatch::Content)
        );
        let prose = parts(&["see ", "@C02", " now"]);
        let edited = parts(&["see ", "@C08", " later"]);
        assert_eq!(
            check(&prose, &edited, "C02", &destinations, false),
            Err(Mismatch::Content)
        );
        let foreign = parts(&["see ", "@C01", " now"]);
        assert_eq!(
            check(&prose, &foreign, "C02", &destinations, false),
            Err(Mismatch::Destination("C01".into()))
        );
        let dropped = parts(&["[", "@C02", "]"]);
        assert_eq!(
            check(&before, &dropped, "C02", &destinations, true),
            Err(Mismatch::Content)
        );
    }
}
