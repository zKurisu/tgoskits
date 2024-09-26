use core::fmt::Display;

use crate::sync::RwLock;
use alloc::{
    collections::btree_map::BTreeMap,
    string::{String, ToString},
};

pub use driver_interface::DriverId;

static MANAGER: RwLock<IdManager> = RwLock::new(IdManager::new());

pub fn driver_id_by_node_name(name: &str) -> DriverId {
    MANAGER.write().id_by_node_name(name)
}
pub fn driver_id_next() -> DriverId {
    let mut g = MANAGER.write();
    g.id_iter += 1;
    g.id_iter.into()
}

struct IdManager {
    id_iter: u64,
    node_name_id_map: BTreeMap<String, DriverId>,
}

impl IdManager {
    const fn new() -> Self {
        Self {
            id_iter: 0,
            node_name_id_map: BTreeMap::new(),
        }
    }

    pub fn id_by_node_name(&mut self, name: &str) -> DriverId {
        *self
            .node_name_id_map
            .entry(name.to_string())
            .or_insert_with(|| {
                self.id_iter += 1;
                self.id_iter.into()
            })
    }
}
