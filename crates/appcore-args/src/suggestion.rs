// =============================================================================
//        #######
//     ###       ###     F: suggestion.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/19 13:34:54 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/19 13:34:54 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

const MAX_SUGGESTION_BYTES: usize = 128;

pub(crate) fn closest<'a>(
    input: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    if input.len() > MAX_SUGGESTION_BYTES {
        return None;
    }
    candidates
        .into_iter()
        .filter(|candidate| candidate.len() <= MAX_SUGGESTION_BYTES)
        .filter_map(|candidate| {
            let distance = edit_distance(input, candidate);
            (distance <= accepted_distance(input.len().max(candidate.len())))
                .then_some((distance, candidate))
        })
        .min_by_key(|(distance, candidate)| (*distance, candidate.len()))
        .map(|(_, candidate)| candidate)
}

pub(crate) fn append(message: String, suggestion: Option<&str>, prefix: &str) -> String {
    match suggestion {
        Some(suggestion) => format!("{message}; did you mean `{prefix}{suggestion}`?"),
        None => message,
    }
}

fn accepted_distance(length: usize) -> usize {
    match length {
        0..=4 => 1,
        5..=8 => 2,
        _ => 3,
    }
}

fn edit_distance(left: &str, right: &str) -> usize {
    let right = right.as_bytes();
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    let mut current = vec![0; right.len() + 1];
    for (left_index, left_byte) in left.bytes().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_byte) in right.iter().enumerate() {
            let substitution = previous[right_index] + usize::from(left_byte != *right_byte);
            current[right_index + 1] = substitution
                .min(previous[right_index + 1] + 1)
                .min(current[right_index] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::closest;

    #[test]
    fn finds_close_candidates_and_bounds_large_input() {
        assert_eq!(closest("publsh", ["publish", "status"]), Some("publish"));
        assert_eq!(closest(&"x".repeat(129), ["publish"]), None);
        assert_eq!(closest("unrelated", ["publish", "status"]), None);
    }
}
