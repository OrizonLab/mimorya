//! Renders contract types as one TypeScript module: each type declared once,
//! with every type it refers to, sorted by name so that the output only
//! changes when the contract does.

use std::any::TypeId;
use std::collections::{BTreeMap, HashSet};

use ts_rs::{Config, TS, TypeVisitor};

const HEADER: &str = "// Generated from the API's Rust types (crates/api/src/contract, ADR-036): do not edit.\n\
// Regenerate with `pnpm contract`.\n";

pub struct Declarations {
    config: Config,
    seen: HashSet<TypeId>,
    /// TypeScript name → (Rust type, declaration).
    declared: BTreeMap<String, (TypeId, String)>,
}

impl Declarations {
    pub fn new() -> Self {
        Self {
            // The API's integers stay far below 2^53: plain numbers in JSON.
            config: Config::new().with_large_int("number"),
            seen: HashSet::new(),
            declared: BTreeMap::new(),
        }
    }

    /// Declares `T` and, recursively, the types it refers to.
    pub fn add<T: TS + 'static + ?Sized>(&mut self) {
        if !self.seen.insert(TypeId::of::<T>()) {
            return;
        }
        // Built-in types (strings, numbers, arrays…) have nothing to declare.
        if T::output_path().is_none() {
            return;
        }
        let name = T::ident(&self.config);
        let mut declaration = T::docs().unwrap_or_default();
        declaration.push_str("export ");
        declaration.push_str(&T::decl(&self.config));
        if let Some((other, _)) = self.declared.get(&name) {
            assert!(
                *other == TypeId::of::<T>(),
                "two contract types are both named {name}"
            );
        }
        self.declared.insert(name, (TypeId::of::<T>(), declaration));
        T::visit_dependencies(self);
    }

    pub fn render(self) -> String {
        let mut out = String::from(HEADER);
        for (_, declaration) in self.declared.into_values() {
            out.push('\n');
            for line in declaration.lines() {
                out.push_str(line.trim_end());
                out.push('\n');
            }
        }
        out
    }
}

impl TypeVisitor for Declarations {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        self.add::<T>();
    }
}
