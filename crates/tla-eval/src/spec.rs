use std::collections::{BTreeMap, BTreeSet};

use tla_syntax::{Def, Expr, Module, Unit, parse_module};

use crate::error::{Error, Result};

/// Where the source of a module named in `EXTENDS` or `INSTANCE` comes from.
///
/// The standard modules are not looked up here: their operators are built in,
/// so `EXTENDS Naturals` needs nothing loaded.
pub trait Modules {
    fn source(&self, name: &str) -> Option<String>;
}

/// No modules beyond the one given and whatever it declares inside itself.
pub struct NoModules;

impl Modules for NoModules {
    fn source(&self, _name: &str) -> Option<String> {
        None
    }
}

/// Modules as `<directory>/<Name>.tla`, which is how TLA+ tools find them.
pub struct Directory(pub std::path::PathBuf);

impl Modules for Directory {
    fn source(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.0.join(format!("{name}.tla"))).ok()
    }
}

/// The standard modules, whose operators this crate implements directly.
const BUILT_IN: &[&str] = &[
    "Naturals",
    "Integers",
    "Reals",
    "Sequences",
    "FiniteSets",
    "TLC",
    "Bags",
    "RealTime",
    "Randomization",
    "Toolbox",
];

/// A specification: a root module together with every module it reaches.
#[derive(Debug)]
pub struct Spec {
    modules: Vec<Module>,
    facts: Vec<Facts>,
    root: usize,
}

/// What was worked out about one module once all of them were loaded.
#[derive(Debug, Default)]
struct Facts {
    /// Declared here or in anything this module extends.
    variables: BTreeSet<String>,
    constants: BTreeSet<String>,
    extends: Vec<usize>,
    instances: Vec<Instance>,
}

#[derive(Debug)]
pub(crate) struct Instance {
    pub(crate) name: Option<String>,
    pub(crate) target: usize,
    /// Every name the target declares, paired with what replaces it. Names the
    /// `WITH` clause leaves out are replaced by the same name here, which is
    /// what TLA+ says an omitted substitution means.
    pub(crate) subs: Vec<(String, Expr)>,
}

impl Spec {
    /// Parse a self-contained specification. `EXTENDS` of a standard module is
    /// honoured; anything else it names is reported as missing.
    pub fn parse(src: &str) -> Result<Self> {
        Self::load(src, &NoModules)
    }

    pub fn load(src: &str, modules: &impl Modules) -> Result<Self> {
        let root = parse_module(src)?;
        let mut builder = Builder {
            modules: Vec::new(),
            index: BTreeMap::new(),
            source: modules,
        };
        let root = builder.add(root)?;
        let modules = builder.modules;
        let facts = resolve(&modules, &builder.index)?;
        Ok(Self {
            modules,
            facts,
            root,
        })
    }

    pub fn name(&self) -> &str {
        &self.modules[self.root].name
    }

    /// The variables of the root module, including any it inherits.
    pub fn variables(&self) -> impl Iterator<Item = &str> {
        self.facts[self.root].variables.iter().map(String::as_str)
    }

    pub fn constants(&self) -> impl Iterator<Item = &str> {
        self.facts[self.root].constants.iter().map(String::as_str)
    }

    pub fn defines(&self, name: &str) -> bool {
        self.definition(self.root, name).is_some()
    }

    pub(crate) fn root(&self) -> usize {
        self.root
    }

    pub(crate) fn declares_variable(&self, module: usize, name: &str) -> bool {
        self.facts[module].variables.contains(name)
    }

    pub(crate) fn declares_constant(&self, module: usize, name: &str) -> bool {
        self.facts[module].constants.contains(name)
    }

    /// Find a definition, following `EXTENDS`. The module it was found in
    /// comes back with it, because that is the scope its body must be read in.
    pub(crate) fn definition(&self, module: usize, name: &str) -> Option<(usize, &Def)> {
        let mut seen = BTreeSet::new();
        self.search(module, name, &mut seen)
    }

    fn search(
        &self,
        module: usize,
        name: &str,
        seen: &mut BTreeSet<usize>,
    ) -> Option<(usize, &Def)> {
        if !seen.insert(module) {
            return None;
        }
        if let Some(def) = self.modules[module].definition(name) {
            return Some((module, def));
        }
        self.facts[module]
            .extends
            .iter()
            .find_map(|&parent| self.search(parent, name, seen))
    }

    /// Find an instance by the name it was given, following `EXTENDS`.
    pub(crate) fn instance(&self, module: usize, name: &str) -> Option<&Instance> {
        let mut seen = BTreeSet::new();
        self.find_instance(module, name, &mut seen)
    }

    fn find_instance(
        &self,
        module: usize,
        name: &str,
        seen: &mut BTreeSet<usize>,
    ) -> Option<&Instance> {
        if !seen.insert(module) {
            return None;
        }
        let here = self.facts[module]
            .instances
            .iter()
            .find(|i| i.name.as_deref() == Some(name));
        if here.is_some() {
            return here;
        }
        self.facts[module]
            .extends
            .iter()
            .find_map(|&parent| self.find_instance(parent, name, seen))
    }
}

struct Builder<'a, M: Modules> {
    modules: Vec<Module>,
    index: BTreeMap<String, usize>,
    source: &'a M,
}

impl<M: Modules> Builder<'_, M> {
    /// Register a module and everything it names, depth first.
    fn add(&mut self, module: Module) -> Result<usize> {
        let name = module.name.clone();
        if let Some(&existing) = self.index.get(&name) {
            return Ok(existing);
        }
        let position = self.modules.len();
        self.index.insert(name, position);
        self.modules.push(module);

        // Inner modules are visible to their parent, and are registered before
        // anything is loaded from outside so they take precedence.
        let inner: Vec<Module> = self.modules[position]
            .units
            .iter()
            .filter_map(|u| match u {
                Unit::Inner(m) => Some((**m).clone()),
                _ => None,
            })
            .collect();
        for module in inner {
            self.add(module)?;
        }

        let mut wanted: Vec<String> = self.modules[position].extends.clone();
        wanted.extend(self.modules[position].units.iter().filter_map(|u| match u {
            Unit::Instance { module, .. } => Some(module.clone()),
            _ => None,
        }));
        for name in wanted {
            if BUILT_IN.contains(&name.as_str()) || self.index.contains_key(&name) {
                continue;
            }
            let Some(src) = self.source.source(&name) else {
                return Err(Error::Undefined(format!(
                    "module `{name}` is neither a standard module nor one that could be found"
                )));
            };
            let parsed = parse_module(&src)?;
            self.add(parsed)?;
        }
        Ok(position)
    }
}

fn resolve(modules: &[Module], index: &BTreeMap<String, usize>) -> Result<Vec<Facts>> {
    let mut facts: Vec<Facts> = modules.iter().map(|_| Facts::default()).collect();

    for (position, module) in modules.iter().enumerate() {
        facts[position].extends = module
            .extends
            .iter()
            .filter_map(|name| index.get(name).copied())
            .collect();
        facts[position].variables = module.variables().cloned().collect();
        facts[position].constants = module.constants().map(|d| d.name.clone()).collect();
    }

    // A module declares whatever it extends declares.
    for position in 0..modules.len() {
        let mut seen = BTreeSet::new();
        let mut inherited = (BTreeSet::new(), BTreeSet::new());
        collect(position, &facts, &mut seen, &mut inherited);
        facts[position].variables.extend(inherited.0);
        facts[position].constants.extend(inherited.1);
    }

    for (position, module) in modules.iter().enumerate() {
        facts[position].instances = module
            .units
            .iter()
            .filter_map(|u| match u {
                Unit::Instance { name, module, subs } => Some((name, module, subs)),
                _ => None,
            })
            .map(|(name, target, subs)| {
                let target = *index.get(target).ok_or_else(|| {
                    Error::Undefined(format!("INSTANCE of unknown module `{target}`"))
                })?;
                Ok(Instance {
                    name: name.clone(),
                    target,
                    subs: complete(&facts[target], subs),
                })
            })
            .collect::<Result<Vec<_>>>()?;
    }
    Ok(facts)
}

fn collect(
    position: usize,
    facts: &[Facts],
    seen: &mut BTreeSet<usize>,
    out: &mut (BTreeSet<String>, BTreeSet<String>),
) {
    for &parent in &facts[position].extends {
        if !seen.insert(parent) {
            continue;
        }
        out.0.extend(facts[parent].variables.iter().cloned());
        out.1.extend(facts[parent].constants.iter().cloned());
        collect(parent, facts, seen, out);
    }
}

/// A `WITH` clause need not mention every declared name; the ones it leaves
/// out keep their names, and so refer to whatever bears that name where the
/// instance was written.
fn complete(target: &Facts, given: &[(String, Expr)]) -> Vec<(String, Expr)> {
    let mut subs = given.to_vec();
    let declared = target.constants.iter().chain(target.variables.iter());
    for name in declared {
        if !subs.iter().any(|(given, _)| given == name) {
            subs.push((name.clone(), Expr::Ident(name.clone())));
        }
    }
    subs
}
