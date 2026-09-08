//! The rail beside a history: which lane a commit sits in, and which lanes
//! run past it.
//!
//! A history is a list, but it is not a line: a merge brings two lines
//! together and a branch point sends one out. Drawing that needs to know,
//! for every row, where the dot goes and which columns have a thread
//! running through them. The rule is the one every git viewer uses — a lane
//! is a sha the next row is waiting for — and it is worked out here, once,
//! away from the window, where it can be tested.

/// Where one commit sits on the rail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The lane the dot goes in, counting from the left.
    pub lane: usize,
    /// Every lane that has a thread through this row, the dot's included.
    /// A lane in this list is drawn as a vertical line.
    pub through: Vec<usize>,
    /// Whether this commit has more than one parent.
    pub merge: bool,
}

/// How many lanes the rail is allowed to grow to. A history wide enough to
/// need more is one nobody reads by its picture, and the rail has to fit
/// beside the message.
pub const MAX_LANES: usize = 6;

/// Lay a history out on the rail.
///
/// `commits` is the list as GitHub sends it, newest first, each with its
/// parents. Lanes are claimed as commits appear and freed when nothing is
/// waiting for them, so a linear history stays in lane zero and a merge
/// opens exactly one more.
pub fn lay_out<'a, I>(commits: I) -> Vec<Row>
where
    I: IntoIterator<Item = (&'a str, &'a [String])>,
{
    // Each lane holds the sha it is waiting to see next.
    let mut lanes: Vec<Option<String>> = Vec::new();
    let mut rows = Vec::new();
    for (sha, parents) in commits {
        // The commit lands in the lane that was waiting for it, or in the
        // first free one.
        let lane = lanes
            .iter()
            .position(|waiting| waiting.as_deref() == Some(sha))
            .or_else(|| lanes.iter().position(Option::is_none))
            .unwrap_or_else(|| {
                lanes.push(None);
                lanes.len() - 1
            });
        if lane >= lanes.len() {
            lanes.resize(lane + 1, None);
        }
        // Any other lane waiting for the same commit has met it here.
        for (other, waiting) in lanes.iter_mut().enumerate() {
            if other != lane && waiting.as_deref() == Some(sha) {
                *waiting = None;
            }
        }
        // The first parent carries this lane on; the rest open their own,
        // unless another lane is already waiting for them.
        lanes[lane] = parents.first().cloned();
        for parent in parents.iter().skip(1) {
            if lanes
                .iter()
                .any(|waiting| waiting.as_deref() == Some(parent.as_str()))
            {
                continue;
            }
            match lanes.iter().position(Option::is_none) {
                Some(free) => lanes[free] = Some(parent.clone()),
                None if lanes.len() < MAX_LANES => lanes.push(Some(parent.clone())),
                None => {}
            }
        }
        let mut through: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(_, waiting)| waiting.is_some())
            .map(|(index, _)| index)
            .collect();
        if !through.contains(&lane) {
            through.push(lane);
            through.sort_unstable();
        }
        rows.push(Row {
            lane,
            through,
            merge: parents.len() > 1,
        });
    }
    rows
}

/// How many lanes a laid-out history uses, which is how wide the rail has
/// to be.
pub fn width(rows: &[Row]) -> usize {
    rows.iter()
        .flat_map(|row| row.through.iter().copied().chain(std::iter::once(row.lane)))
        .max()
        .map(|last| last + 1)
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(pairs: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
        pairs
            .iter()
            .map(|(sha, parents)| {
                (
                    sha.to_string(),
                    parents.iter().map(|p| p.to_string()).collect(),
                )
            })
            .collect()
    }

    fn lay(pairs: &[(&str, &[&str])]) -> Vec<Row> {
        let owned = history(pairs);
        lay_out(
            owned
                .iter()
                .map(|(sha, parents)| (sha.as_str(), parents.as_slice())),
        )
    }

    #[test]
    fn a_straight_history_stays_in_one_lane() {
        let rows = lay(&[("c", &["b"]), ("b", &["a"]), ("a", &[])]);
        assert_eq!(
            rows.iter().map(|row| row.lane).collect::<Vec<_>>(),
            [0, 0, 0]
        );
        assert!(rows.iter().all(|row| !row.merge));
        assert_eq!(width(&rows), 1);
    }

    #[test]
    fn a_merge_opens_a_lane_and_meeting_again_closes_it() {
        // m merges the side branch s back into the trunk t.
        let rows = lay(&[
            ("m", &["t", "s"]),
            ("t", &["base"]),
            ("s", &["base"]),
            ("base", &[]),
        ]);
        assert!(rows[0].merge);
        assert_eq!(rows[0].lane, 0);
        assert_eq!(rows[0].through, [0, 1], "the side branch runs beside it");
        assert_eq!(rows[1].lane, 0, "the first parent carries the lane on");
        assert_eq!(rows[2].lane, 1, "the second parent took the lane beside it");
        assert_eq!(
            rows[3].lane, 0,
            "both met at the base, which is back in one"
        );
        assert_eq!(width(&rows), 2);
    }

    #[test]
    fn the_last_commit_leaves_nothing_running() {
        let rows = lay(&[("only", &[])]);
        assert_eq!(rows[0].through, [0]);
        assert_eq!(width(&rows), 1);
    }

    #[test]
    fn nothing_lays_out_as_nothing() {
        let rows = lay(&[]);
        assert!(rows.is_empty());
        assert_eq!(width(&rows), 1, "an empty rail is still one lane wide");
    }
}
