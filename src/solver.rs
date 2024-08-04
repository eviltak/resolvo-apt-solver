use resolvo::utils::{Pool, Range};
use resolvo::{
    Candidates, Dependencies, DependencyProvider, Interner, KnownDependencies, Mapping, NameId,
    SolvableId, Solver, SolverCache, StringId, UnsolvableOrCancelled, VersionSetId,
};

use std::fmt::{Display, Write};

use apt_edsp::scenario::{Package, Relation, Relationship, Scenario};
use apt_edsp::Version;

pub struct DebProvider<'s> {
    pool: Pool<Range<&'s Version>, &'s str>,
    candidates: Mapping<NameId, Vec<SolvableId>>,
    packages: Mapping<SolvableId, &'s Package>,
}

impl<'s> DebProvider<'s> {
    fn new(scenario: &'s Scenario) -> Self {
        let pool = Pool::default();
        let mut candidates = Mapping::default();
        let mut packages = Mapping::default();

        for package in &scenario.universe {
            // Add real package
            let real_name = pool.intern_package_name(&*package.package);
            let real_solvable = pool.intern_solvable(real_name, &package.version);

            // Add to candidates mapping
            match candidates.get_mut(real_name) {
                None => candidates.insert(real_name, vec![real_solvable]),
                Some(candidates) => candidates.push(real_solvable),
            }

            packages.insert(real_solvable, package);

            // TODO: virtual packages
        }

        DebProvider {
            pool,
            candidates,
            packages,
        }
    }

    fn intern_relationship_as_version_set(&self, relationship: &'s Relationship) -> VersionSetId {
        self.intern_version_set(
            &*relationship.package,
            constraint_to_version_set(&relationship.constraint),
        )
    }

    fn intern_version_set(&self, name: &'s str, version_set: Range<&'s Version>) -> VersionSetId {
        self.pool
            .intern_version_set(self.pool.intern_package_name(name), version_set)
    }
}

impl<'s> Interner for DebProvider<'s> {
    fn display_solvable(&self, solvable: SolvableId) -> impl Display + '_ {
        let solvable = self.pool.resolve_solvable(solvable);
        format!("{}={}", self.display_name(solvable.name), solvable.record)
    }

    fn display_merged_solvables(&self, solvables: &[SolvableId]) -> impl Display + '_ {
        if solvables.is_empty() {
            return "".to_string();
        }

        let name = self.display_name(self.pool.resolve_solvable(solvables[0]).name);

        let mut versions = solvables
            .iter()
            .map(|&s| self.pool.resolve_solvable(s).record);

        let mut buf = format!("{name} {}", versions.next().unwrap());

        for version in versions {
            write!(&mut buf, " | {version}").expect("buffer write error");
        }

        buf
    }

    fn display_name(&self, name: NameId) -> impl Display + '_ {
        self.pool.resolve_package_name(name)
    }

    fn display_version_set(&self, version_set: VersionSetId) -> impl Display + '_ {
        self.pool.resolve_version_set(version_set)
    }

    fn display_string(&self, string_id: StringId) -> impl Display + '_ {
        self.pool.resolve_string(string_id)
    }

    fn version_set_name(&self, version_set: VersionSetId) -> NameId {
        self.pool.resolve_version_set_package_name(version_set)
    }

    fn solvable_name(&self, solvable: SolvableId) -> NameId {
        self.pool.resolve_solvable(solvable).name
    }
}

impl<'s> DependencyProvider for DebProvider<'s> {
    async fn filter_candidates(
        &self,
        candidates: &[SolvableId],
        version_set: VersionSetId,
        inverse: bool,
    ) -> Vec<SolvableId> {
        let range = self.pool.resolve_version_set(version_set);
        candidates
            .iter()
            .copied()
            .filter(|s| range.contains(&self.pool.resolve_solvable(*s).record) == !inverse)
            .collect()
    }

    async fn get_candidates(&self, name: NameId) -> Option<Candidates> {
        let candidates = self.candidates.get(name)?;

        Some(Candidates {
            candidates: candidates.clone(),
            favored: candidates.iter().copied().find(|&solvable| {
                self.packages
                    .get(solvable)
                    .map(|p| p.installed.0)
                    .unwrap_or(false)
            }),
            // TODO: Lock to apt candidate if strict pinning
            locked: None,
            // We already have all the dependency information in memory
            // hint_dependencies_available: candidates.clone(),
            hint_dependencies_available: vec![],
            // TODO: Exclude based on architecture
            excluded: vec![],
        })
    }

    async fn sort_candidates(&self, _solver: &SolverCache<Self>, solvables: &mut [SolvableId]) {
        solvables.sort_by(|a, b| {
            let a = self.pool.resolve_solvable(*a).record;
            let b = self.pool.resolve_solvable(*b).record;
            // TODO: Consider pin priority
            b.cmp(&a)
        });
    }

    async fn get_dependencies(&self, solvable: SolvableId) -> Dependencies {
        let Some(package) = self.packages.get(solvable) else {
            let reason = self.pool.intern_string("Unknown package");
            return Dependencies::Unknown(reason);
        };

        // TODO: Use predepends as well
        let requirements = package
            .depends
            .iter()
            // TODO: Handle alternate dependencies (will likely require a resolvo change)
            .map(|dep| self.intern_relationship_as_version_set(&dep.first))
            .collect();

        // Specify conflicts by constraining to complement of conflicting set
        // TODO: Use breaks as well
        let constrains = package
            .conflicts
            .iter()
            .map(|rel| {
                self.intern_version_set(
                    &*rel.package,
                    constraint_to_version_set(&rel.constraint).complement(),
                )
            })
            .collect();

        Dependencies::Known(KnownDependencies {
            requirements,
            constrains,
        })
    }
}

fn constraint_to_version_set(constraint: &Option<(Relation, Version)>) -> Range<&Version> {
    match constraint {
        None => Range::full(),
        Some((relation, version)) => match relation {
            Relation::Earlier => Range::strictly_lower_than(version),
            Relation::EarlierEqual => Range::lower_than(version),
            Relation::Equal => Range::singleton(version),
            Relation::LaterEqual => Range::higher_than(version),
            Relation::Later => Range::strictly_higher_than(version),
        },
    }
}

pub fn solve(
    scenario: &Scenario,
) -> (
    Result<Vec<SolvableId>, UnsolvableOrCancelled>,
    Solver<DebProvider>,
) {
    let provider = DebProvider::new(scenario);

    // TODO: Handle Autoremove, Upgrade-All Forbid-Remove and Forbid-New-Install
    let requirements = scenario
        .request
        .actions
        .install
        .as_deref()
        .unwrap_or("")
        .split_ascii_whitespace()
        .map(|package| provider.intern_version_set(package, Range::full()))
        .collect();

    let constraints = scenario
        .request
        .actions
        .remove
        .as_deref()
        .unwrap_or("")
        .split_ascii_whitespace()
        .map(|package| provider.intern_version_set(package, Range::empty()))
        .collect();

    let installed_packages = provider
        .packages
        .iter()
        .filter(|(_, package)| package.installed.0)
        .map(|(solvable_id, _)| solvable_id)
        .collect::<Vec<_>>();

    let mut solver = Solver::new(provider);
    // TODO: Use installed packages as soft requirements
    let result = solver.solve(requirements, constraints);

    (result, solver)
}
