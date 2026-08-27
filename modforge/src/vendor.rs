//! Engine-independent vendor offer planning.
//!
//! Games supply item identifiers, current assignments, and price snapshots.
//! The planner assigns each new item once and returns offers for the engine
//! layer to apply.

use std::collections::{HashMap, HashSet};

/// One item observed by a game-specific vendor adapter.
pub struct VendorItem<I> {
    pub name: String,
    pub id: I,
}

/// One offer for the game-specific vendor adapter to apply.
pub struct VendorOffer<I> {
    pub name: String,
    pub id: I,
    pub price: Option<i32>,
    pub stock: Option<i32>,
}

/// Assigns candidate items globally and calculates percentage prices.
pub struct OfferPlanner {
    assigned: HashSet<String>,
    price_percent: i32,
}

impl OfferPlanner {
    pub fn new(assigned: impl IntoIterator<Item = String>, price_percent: i32) -> Self {
        Self {
            assigned: assigned.into_iter().collect(),
            price_percent,
        }
    }

    /// Plan offers from a vendor inventory or caller-selected item set.
    /// Assignments remain provisional until [`commit`](Self::commit), so a
    /// failed engine mutation does not prevent another vendor taking them.
    pub fn plan<I>(
        &self,
        items: impl IntoIterator<Item = VendorItem<I>>,
        costs: &HashMap<String, i32>,
    ) -> Vec<VendorOffer<I>> {
        items
            .into_iter()
            .filter(|item| !self.assigned.contains(&item.name))
            .map(|item| VendorOffer {
                price: costs
                    .get(&item.name)
                    .map(|cost| (cost * self.price_percent / 100).max(1)),
                name: item.name,
                id: item.id,
                stock: None,
            })
            .collect()
    }

    /// Make successfully applied offers unavailable to later vendors.
    pub fn commit<I>(&mut self, offers: &[VendorOffer<I>]) {
        self.assigned
            .extend(offers.iter().map(|offer| offer.name.clone()));
    }
}

/// Return a caller-supplied special offer unless that vendor already has it.
pub fn special_offer<I>(
    existing: &HashSet<String>,
    item: VendorItem<I>,
    price: Option<i32>,
    stock: Option<i32>,
) -> Option<VendorOffer<I>> {
    if existing.contains(item.name.as_str()) {
        return None;
    }
    Some(VendorOffer {
        name: item.name,
        id: item.id,
        price,
        stock,
    })
}
