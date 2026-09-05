//! The example domain module for v6-project.

/// One item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Its name.
    pub name: String,
}

/// Every item this build knows about.
#[must_use]
pub fn all() -> Vec<Item> {
    crate::seed::items()
}

#[cfg(test)]
mod tests {
    #[test]
    fn all_items_are_listable() {
        let _ = super::all();
    }
}
