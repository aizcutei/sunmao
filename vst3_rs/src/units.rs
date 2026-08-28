//! Turns `/`-separated parameter group paths into a VST3 unit tree.
//!
//! VST3 does not take a path per parameter the way CLAP does. It expects a tree
//! of units, each with an id and a parent id, and every parameter carries the
//! id of the unit it belongs to. This module derives that tree from the paths
//! so a plugin declares the hierarchy once.

use crate::vst3_sys::vst::ivstunits::{kNoParentUnitId, kRootUnitId};
use crate::vst3_sys::vst::types::UnitID;

/// One unit as VST3 wants to see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub id: UnitID,
    pub parent_id: UnitID,
    /// Just this level's name, not the whole path — VST3 shows the tree.
    pub name: String,
}

/// The unit tree plus the mapping a parameter needs.
#[derive(Debug, Clone, Default)]
pub struct UnitTable {
    /// Root first, then one entry per distinct path, parents before children.
    units: Vec<Unit>,
    /// Full path -> unit id.
    paths: Vec<(String, UnitID)>,
}

impl UnitTable {
    /// Builds the tree from each parameter's group path, in declaration order.
    ///
    /// Intermediate levels are created even when no parameter names them
    /// directly: a lone `"Osc/Tuning"` still needs an `Osc` unit to hang from,
    /// or the host would be handed a unit whose parent does not exist.
    pub fn from_paths<'a>(paths: impl IntoIterator<Item = &'a str>) -> Self {
        let mut table = Self {
            units: vec![Unit {
                id: kRootUnitId,
                parent_id: kNoParentUnitId,
                name: "Root".to_string(),
            }],
            paths: Vec::new(),
        };

        for path in paths {
            let mut parent = kRootUnitId;
            let mut prefix = String::new();
            for segment in path.split('/').filter(|s| !s.is_empty()) {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(segment);

                parent = match table.id_for_path(&prefix) {
                    Some(existing) => existing,
                    None => {
                        // Ids start at 1: 0 is the root.
                        let id = table.units.len() as UnitID;
                        table.units.push(Unit {
                            id,
                            parent_id: parent,
                            name: segment.to_string(),
                        });
                        table.paths.push((prefix.clone(), id));
                        id
                    }
                };
            }
        }

        table
    }

    fn id_for_path(&self, path: &str) -> Option<UnitID> {
        self.paths
            .iter()
            .find(|(known, _)| known == path)
            .map(|(_, id)| *id)
    }

    /// The unit a parameter with this group path belongs to.
    ///
    /// An unknown or empty path maps to the root, which is what a parameter
    /// with no declared group should report.
    pub fn unit_for(&self, path: &str) -> UnitID {
        let normalized: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if normalized.is_empty() {
            return kRootUnitId;
        }
        self.id_for_path(&normalized.join("/"))
            .unwrap_or(kRootUnitId)
    }

    pub fn units(&self) -> &[Unit] {
        &self.units
    }

    /// Whether anything beyond the root exists. A plugin with no groups should
    /// not advertise `IUnitInfo` at all.
    pub fn has_groups(&self) -> bool {
        self.units.len() > 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flat_plugin_has_only_the_root_unit() {
        let table = UnitTable::from_paths(["", "", ""]);
        assert_eq!(table.units().len(), 1);
        assert!(!table.has_groups());
        assert_eq!(table.unit_for(""), kRootUnitId);
    }

    #[test]
    fn each_distinct_path_becomes_one_unit() {
        let table = UnitTable::from_paths(["Osc", "Osc", "Filter", ""]);
        assert!(table.has_groups());
        // Root + Osc + Filter, and the repeat did not create a second Osc.
        assert_eq!(table.units().len(), 3);
        assert_ne!(table.unit_for("Osc"), table.unit_for("Filter"));
        assert_eq!(table.unit_for(""), kRootUnitId);
    }

    #[test]
    fn intermediate_levels_are_created_even_when_unnamed() {
        // Nothing declares "Osc" on its own, but the tree still needs it.
        let table = UnitTable::from_paths(["Osc/Tuning"]);
        let osc = table.unit_for("Osc");
        let tuning = table.unit_for("Osc/Tuning");
        assert_ne!(osc, kRootUnitId, "the intermediate level must exist");
        assert_ne!(tuning, osc);

        let tuning_unit = table
            .units()
            .iter()
            .find(|unit| unit.id == tuning)
            .expect("declared");
        assert_eq!(tuning_unit.parent_id, osc);
        assert_eq!(
            tuning_unit.name, "Tuning",
            "VST3 shows one level, not the path"
        );
    }

    #[test]
    fn every_parent_is_declared_before_its_children() {
        // A host reading the list in order must never see a parent id it has
        // not been told about yet.
        let table = UnitTable::from_paths(["A/B/C", "D/E", "A/F"]);
        let mut seen = vec![kRootUnitId];
        for unit in table.units().iter().skip(1) {
            assert!(
                seen.contains(&unit.parent_id),
                "unit {} claims undeclared parent {}",
                unit.id,
                unit.parent_id
            );
            seen.push(unit.id);
        }
    }

    #[test]
    fn stray_slashes_do_not_create_unnamed_levels() {
        let table = UnitTable::from_paths(["/Filter//Cutoff/"]);
        assert_eq!(table.units().len(), 3, "root + Filter + Cutoff");
        assert!(table.units().iter().all(|unit| !unit.name.is_empty()));
        // And the normalized lookup finds the same unit.
        assert_eq!(
            table.unit_for("Filter/Cutoff"),
            table.unit_for("/Filter//Cutoff/")
        );
    }

    #[test]
    fn an_undeclared_path_falls_back_to_the_root() {
        let table = UnitTable::from_paths(["Osc"]);
        assert_eq!(table.unit_for("Nope"), kRootUnitId);
    }
}
