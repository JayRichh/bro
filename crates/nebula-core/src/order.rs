//! The user's order rule, as every client lays rows out by it (the
//! [`crate::ReorderScope`] the daemon writes): a scope the user has ordered
//! lays out by `sort_order`; one never ordered keeps each view's own order.
//! A scope has a user order once some row's `sort_order` is above 0
//! ([`has_user_order`]): the daemon leaves every worktree and session at 0
//! until a `Reorder` writes 0..n, and numbers projects 0..n as they are
//! added.
//!
//! The board's lanes keep their own order across checkouts and the user's
//! among one checkout's cards ([`refill`]). The base order is the caller's:
//! the terminal app's grid recency, the desktop's newest status change.

use crate::entities::Agent;
use crate::ids::WorktreeId;
use std::collections::HashSet;

/// Whether a scope's rows carry a user order.
pub fn has_user_order(orders: impl IntoIterator<Item = i64>) -> bool {
    orders.into_iter().any(|o| o > 0)
}

/// The checkouts whose sessions carry a user order, archived ones counted.
pub fn ordered_worktrees<'a>(agents: impl IntoIterator<Item = &'a Agent>) -> HashSet<WorktreeId> {
    agents
        .into_iter()
        .filter(|a| a.sort_order > 0)
        .map(|a| a.worktree_id.clone())
        .collect()
}

/// A lane's cards: `cards` in the board's own order, and the user's order
/// among the cards of a checkout in `ordered`. Each such checkout's cards
/// take back the slots they hold, in `sort_order`, so the checkouts stay
/// interleaved as the board's order put them. `key` reads a card's
/// checkout and `sort_order`.
pub fn refill<T>(
    cards: Vec<T>,
    ordered: &HashSet<WorktreeId>,
    key: impl Fn(&T) -> (&WorktreeId, i64),
) -> Vec<T> {
    let mut slots: Vec<(&WorktreeId, Vec<usize>)> = Vec::new();
    for (i, card) in cards.iter().enumerate() {
        let (w, _) = key(card);
        if !ordered.contains(w) {
            continue;
        }
        match slots.iter_mut().find(|(id, _)| *id == w) {
            Some((_, at)) => at.push(i),
            None => slots.push((w, vec![i])),
        }
    }
    // Where each slot's card comes from; a checkout's cards, taken in slot
    // order and sorted stably by `sort_order`, fill its own slots.
    let mut from: Vec<usize> = (0..cards.len()).collect();
    for (_, at) in slots {
        let mut mine = at.clone();
        mine.sort_by_key(|&i| key(&cards[i]).1);
        for (slot, src) in at.into_iter().zip(mine) {
            from[slot] = src;
        }
    }
    let mut taken: Vec<Option<T>> = cards.into_iter().map(Some).collect();
    from.into_iter()
        .map(|i| taken[i].take().expect("each card fills one slot"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scope_is_ordered_once_a_row_is_above_zero() {
        assert!(!has_user_order([0, 0]));
        assert!(has_user_order([0, 1]));
        assert!(!has_user_order([]));
    }

    /// An ordered checkout's cards take back their own slots in
    /// `sort_order`; the rest stay where the board put them.
    type Card = (&'static str, WorktreeId, i64);

    fn key(c: &Card) -> (&WorktreeId, i64) {
        (&c.1, c.2)
    }

    #[test]
    fn refill_keeps_the_board_order_across_checkouts() {
        let w = |s: &str| WorktreeId(s.into());
        // (id, worktree, sort_order), in the board's order.
        let cards = vec![
            ("a1", w("A"), 3),
            ("b1", w("B"), 0),
            ("a2", w("A"), 1),
            ("a3", w("A"), 2),
        ];
        let ids = |v: Vec<Card>| v.into_iter().map(|c| c.0).collect::<Vec<_>>();
        assert_eq!(
            ids(refill(cards.clone(), &HashSet::new(), key)),
            ["a1", "b1", "a2", "a3"]
        );
        let only_a: HashSet<WorktreeId> = [w("A")].into();
        assert_eq!(ids(refill(cards, &only_a, key)), ["a2", "b1", "a3", "a1"]);
    }
}
