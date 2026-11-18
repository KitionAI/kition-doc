//! Ordered lists: making a replica list hold exactly the objects a view holds with as few
//! operations as possible ([`sync_list`]), and pairing up the elements of two versions of a list
//! whose elements have no ids of their own ([`align`]: revisions, table rows and cells).

use std::collections::{HashMap, HashSet};

use kition_kit::kdm::{ObjectId, Tx};

use crate::KdmResult;

/// Makes `owner.field` list exactly `target` (distinct objects): entries not in `target`, and
/// repeated entries, are removed (their objects are not deleted: the caller decides that); the
/// entries already there keep their place as far as possible (the longest run already in
/// `target`'s order stays put) and the rest are moved; new entries are inserted. So moving one
/// element is one `list_move`. Answers how many list operations it wrote.
pub(crate) fn sync_list(tx: &mut Tx<'_>, owner: &ObjectId, field: &str, target: &[ObjectId]) -> KdmResult<usize> {
    let mut current = tx.list_items(owner, field)?;
    if current == target {
        return Ok(0);
    }
    let wanted: HashSet<&ObjectId> = target.iter().collect();
    let mut ops = 0;
    let mut seen = HashSet::new();
    let keep: Vec<bool> = current.iter().map(|o| wanted.contains(o) && seen.insert(o.clone())).collect();
    for index in (0..current.len()).rev() {
        if !keep[index] {
            tx.list_remove(owner, field, index)?;
            current.remove(index);
            ops += 1;
        }
    }
    // Target positions of what is already there, in its current order; the longest increasing
    // run of them stays.
    let position: HashMap<&ObjectId, usize> = target.iter().enumerate().map(|(i, o)| (o, i)).collect();
    let order: Vec<usize> = current.iter().filter_map(|o| position.get(o).copied()).collect();
    let stays: HashSet<usize> = longest_increasing(&order).into_iter().collect();
    for (j, object) in target.iter().enumerate() {
        if stays.contains(&j) {
            continue;
        }
        let after = match j.checked_sub(1).and_then(|p| target.get(p)) {
            Some(prev) => current.iter().position(|o| o == prev).map_or(0, |i| i + 1),
            None => 0,
        };
        match current.iter().position(|o| o == object) {
            Some(from) => {
                let to = if from < after { after - 1 } else { after };
                if from != to {
                    tx.list_move(owner, field, from, to)?;
                    let moved = current.remove(from);
                    current.insert(to, moved);
                    ops += 1;
                }
            }
            None => {
                tx.list_insert(owner, field, after, object)?;
                current.insert(after, object.clone());
                ops += 1;
            }
        }
    }
    Ok(ops)
}

/// The values of one longest strictly increasing subsequence of `values`.
pub(crate) fn longest_increasing(values: &[usize]) -> Vec<usize> {
    // tails[k]: index into `values` of the smallest tail of an increasing run of length k + 1.
    let mut tails: Vec<usize> = Vec::new();
    let mut prev: Vec<Option<usize>> = vec![None; values.len()];
    for (i, &v) in values.iter().enumerate() {
        let k = tails.partition_point(|&t| values[t] < v);
        prev[i] = k.checked_sub(1).map(|p| tails[p]);
        if k == tails.len() {
            tails.push(i);
        } else {
            tails[k] = i;
        }
    }
    let mut out = Vec::with_capacity(tails.len());
    let mut at = tails.last().copied();
    while let Some(i) = at {
        out.push(values[i]);
        at = prev[i];
    }
    out.reverse();
    out
}

/// Pairs the elements of `before` and `after` (each with a value and a key): first the longest
/// common subsequence of equal values (an element this edit did not touch), then, between
/// consecutive pairs of that, the remaining elements with equal keys in order (an element this edit
/// changed). Answers, for each `after` element, its `before` partner.
pub(crate) fn align<V: PartialEq, K: PartialEq>(before: &[(V, K)], after: &[(V, K)]) -> Vec<Option<usize>> {
    let (n, m) = (before.len(), after.len());
    let mut partner: Vec<Option<usize>> = vec![None; m];
    let mut anchors: Vec<(usize, usize)> = Vec::new();
    // O(n·m): capped, as the lists are file data.
    if n.saturating_mul(m) <= 1_000_000 {
        let mut dp = vec![vec![0u32; m + 1]; n + 1];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                dp[i][j] = if before[i].0 == after[j].0 { dp[i + 1][j + 1] + 1 } else { dp[i + 1][j].max(dp[i][j + 1]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n && j < m {
            if before[i].0 == after[j].0 {
                anchors.push((i, j));
                i += 1;
                j += 1;
            } else if dp[i + 1][j] >= dp[i][j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    let mut start = (0, 0);
    for &(i, j) in anchors.iter().chain(std::iter::once(&(n, m))) {
        let mut free: Vec<usize> = (start.0..i).collect();
        for b in start.1..j {
            if let Some(k) = free.iter().position(|&a| before[a].1 == after[b].1) {
                partner[b] = Some(free.remove(k));
            }
        }
        if i < n && j < m {
            partner[j] = Some(i);
        }
        start = (i + 1, j + 1);
    }
    partner
}

/// Makes `owner.field` hold exactly the objects of `target`, in no particular order: entries not
/// in `target` (and repeated entries) are removed, missing ones appended, the rest left where they
/// are. For lists whose order means nothing (parts, comments, media): each view keys them by its
/// own local ids, and making the list follow that order would have peers moving entries back and
/// forth. Answers how many list operations it wrote.
pub(crate) fn sync_set(tx: &mut Tx<'_>, owner: &ObjectId, field: &str, target: &[ObjectId]) -> KdmResult<usize> {
    let current = tx.list_items(owner, field)?;
    let wanted: HashSet<&ObjectId> = target.iter().collect();
    let mut ops = 0;
    let mut seen = HashSet::new();
    let keep: Vec<bool> = current.iter().map(|o| wanted.contains(o) && seen.insert(o.clone())).collect();
    for index in (0..current.len()).rev() {
        if !keep[index] {
            tx.list_remove(owner, field, index)?;
            ops += 1;
        }
    }
    let mut len = seen.len();
    for object in target {
        if seen.insert(object.clone()) {
            tx.list_insert(owner, field, len, object)?;
            len += 1;
            ops += 1;
        }
    }
    Ok(ops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_kit::kdm::{ActorId, DocId, Origin, Replica};
    use serde_json::{Map, json};

    fn replica() -> Replica {
        Replica::create(
            <crate::DocModel as kition_kit::session::KitModel>::schema(),
            DocId::new(),
            ActorId::from_u64(1).unwrap_or_else(|_| panic!("actor")),
        )
        .unwrap_or_else(|error| panic!("create: {error}"))
    }

    fn with_list(n: usize) -> (Replica, Vec<ObjectId>) {
        let mut r = replica();
        let root = crate::root_id();
        let objects = r
            .transact(&Origin::system("t"), |tx| {
                tx.create_with_id("Document", &root, Map::new())?;
                let mut out = Vec::new();
                for i in 0..n {
                    let o = tx.create("Block", [("kind".to_owned(), json!("para"))].into_iter().collect())?;
                    tx.list_insert(&root, "body", i, &o)?;
                    out.push(o);
                }
                Ok(out)
            })
            .unwrap_or_else(|error| panic!("{error}"));
        (r, objects)
    }

    fn sync(r: &mut Replica, target: &[ObjectId]) -> usize {
        let root = crate::root_id();
        let ops = r.transact(&Origin::system("t"), |tx| sync_list(tx, &root, "body", target)).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(r.list(&root, "body").unwrap_or_default(), target);
        ops
    }

    #[test]
    fn one_move_is_one_operation() {
        let (mut r, o) = with_list(5);
        assert_eq!(sync(&mut r, &[o[1].clone(), o[2].clone(), o[3].clone(), o[4].clone(), o[0].clone()]), 1);
        assert_eq!(sync(&mut r, &[o[0].clone(), o[1].clone(), o[2].clone(), o[3].clone(), o[4].clone()]), 1);
        assert_eq!(sync(&mut r, &[o[0].clone(), o[3].clone(), o[1].clone(), o[2].clone(), o[4].clone()]), 1);
        assert_eq!(sync(&mut r, &[o[4].clone(), o[3].clone(), o[2].clone(), o[1].clone(), o[0].clone()]), 3);
        assert_eq!(sync(&mut r, &[o[4].clone(), o[3].clone(), o[2].clone(), o[1].clone(), o[0].clone()]), 0);
    }

    #[test]
    fn sync_set_keeps_order_and_appends() {
        let (mut r, o) = with_list(4);
        let root = crate::root_id();
        let fresh = r
            .transact(&Origin::system("t"), |tx| tx.create("Block", [("kind".to_owned(), json!("para"))].into_iter().collect()))
            .unwrap_or_else(|error| panic!("{error}"));
        let target = [fresh.clone(), o[3].clone(), o[0].clone()];
        let ops = r.transact(&Origin::system("t"), |tx| sync_set(tx, &root, "body", &target)).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(ops, 3);
        assert_eq!(r.list(&root, "body").unwrap_or_default(), vec![o[0].clone(), o[3].clone(), fresh]);
    }

    #[test]
    fn align_pairs_untouched_then_changed_elements() {
        let before = [(1, ()), (2, ()), (3, ())];
        let after = [(1, ()), (9, ()), (2, ()), (30, ())];
        assert_eq!(align(&before, &after), vec![Some(0), None, Some(1), Some(2)]);
    }

    #[test]
    fn longest_increasing_run() {
        assert_eq!(longest_increasing(&[3, 0, 1, 2]), vec![0, 1, 2]);
        assert_eq!(longest_increasing(&[]), Vec::<usize>::new());
        assert_eq!(longest_increasing(&[2, 1, 0]).len(), 1);
    }
}
