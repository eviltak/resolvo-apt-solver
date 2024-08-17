use apt_edsp::answer::Action;
use apt_edsp::scenario::{Request, Scenario};
use apt_edsp::Bool;

use super::*;

const ARCH: &'static str = "amd64";

macro_rules! package {
    ($name:literal $(: $arch:literal)? = $version:literal $(, installed = $installed:literal)? $(, deps = [$($dep:literal),*])? $(, conflicts = [$($conflict:literal),*])?) => {
        TestPackage {
            package: apt_edsp::scenario::Package {
                package: $name.into(),
                version: $version.try_into().unwrap(),
                architecture: Some(ARCH.into()).map(#[allow(unreachable_code)] |a| { $(return $arch.into();)? a }).unwrap(),
                installed: None.or_else(#[allow(unreachable_code)] || { $(return Some($installed.into());)? None }).unwrap_or(Bool::default()),
                $(depends: vec![$($dep.try_into().unwrap()),*],)?
                $(conflicts: vec![$($conflict.try_into().unwrap()),*],)?
                ..Default::default()
            },
            expected_action: None,
        }
    };
}

enum ExpectedAction {
    Install,
    Remove,
    Autoremove,
}

struct TestPackage {
    package: Package,
    expected_action: Option<ExpectedAction>,
}

impl TestPackage {
    fn must(mut self, expected_action: ExpectedAction) -> Self {
        self.expected_action = Some(expected_action);
        self
    }

    fn expected_action(&self) -> Option<Action> {
        self.expected_action
            .as_ref()
            .map(|expected_action| match expected_action {
                ExpectedAction::Install => self.package.to_install().into(),
                ExpectedAction::Remove => self.package.to_remove().into(),
                ExpectedAction::Autoremove => self.package.to_autoremove().into(),
            })
    }
}

struct TestScenario {
    scenario: Scenario,
    expected_actions: Vec<Action>,
}

impl TestScenario {
    fn with_arch(architecture: &str) -> Self {
        Self {
            scenario: Scenario {
                request: Request {
                    request: "EDSP 0.5".into(),
                    architecture: architecture.into(),
                    ..Default::default()
                },
                universe: Default::default(),
            },
            expected_actions: Default::default(),
        }
    }

    fn new() -> Self {
        Self::with_arch(ARCH)
    }

    fn install(mut self, s: impl Into<String>) -> Self {
        self.scenario.request.actions.install = Some(s.into());
        self
    }

    fn remove(mut self, s: impl Into<String>) -> Self {
        self.scenario.request.actions.remove = Some(s.into());
        self
    }

    fn packages(mut self, packages: impl IntoIterator<Item = TestPackage>) -> Self {
        for mut test_package in packages {
            test_package.package.id = self.scenario.universe.len().to_string();
            self.expected_actions.extend(test_package.expected_action());
            self.scenario.universe.push(test_package.package);
        }

        self
    }

    fn check_solution(mut self) {
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::TRACE)
                .finish(),
        );

        let mut answer = solve(&self.scenario);

        fn action_id_key(action: &Action) -> &str {
            match action {
                Action::Install(install) => &install.install,
                Action::Remove(remove) => &remove.remove,
                Action::Autoremove(autoremove) => &autoremove.autoremove,
            }
        }

        fn action_id_cmp(x: &Action, y: &Action) -> std::cmp::Ordering {
            action_id_key(x).cmp(action_id_key(y))
        }

        if let Answer::Solution(ref mut actions) = answer {
            actions.sort_unstable_by(action_id_cmp);
        }

        self.expected_actions.sort_unstable_by(action_id_cmp);

        assert_eq!(Answer::Solution(self.expected_actions), answer);
    }
}

use ExpectedAction::*;

#[test]
fn simple() {
    TestScenario::new()
        .install("baz")
        .packages([
            package!("foo" = "1").must(Install),
            package!("foo" = "2"),
            package!("foo" = "3"),
            package!("bar" = "0", conflicts = ["foo (>= 2)"]).must(Install),
            package!("baz" = "0", deps = ["foo", "bar"]).must(Install),
        ])
        .check_solution();
}

#[test]
fn request_conflicts_with_installed() {
    TestScenario::new()
        .install("bar")
        .packages([
            package!("foo" = "0", conflicts = ["qux"]),
            package!("foo" = "1"),
            package!("foo" = "2", conflicts = ["qux"]),
            package!("foo" = "3", installed = true).must(Remove),
            package!("bar" = "0", conflicts = ["foo (>= 2)"]).must(Install),
            package!("qux" = "0", installed = true),
        ])
        .check_solution();
}

#[test]
fn installed_conflicts_with_request() {
    TestScenario::new()
        .install("bar")
        .packages([
            package!("foo" = "0", conflicts = ["bar"]),
            package!("foo" = "1"),
            package!("foo" = "2", conflicts = ["bar"]),
            package!("foo" = "3", installed = true, conflicts = ["bar"]).must(Remove),
            package!("bar" = "0").must(Install),
            package!("qux" = "0", installed = true),
        ])
        .check_solution();
}

#[test]
fn old_dependency_installed() {
    TestScenario::new()
        .install("bar")
        .packages([
            package!("foo" = "1"),
            package!("foo" = "2", installed = true),
            package!("foo" = "3"),
            package!("bar" = "0", deps = ["foo"]).must(Install),
        ])
        .check_solution();
}

#[test]
fn installed_depends_on_older_version() {
    TestScenario::new()
        .install("baz")
        .packages([
            package!("foo" = "1", installed = true),
            package!("foo" = "2").must(Install),
            package!("bar" = "0", installed = true, deps = ["foo (= 1)"]).must(Remove),
            package!("baz" = "0", deps = ["foo (= 2)"]).must(Install),
        ])
        .check_solution();
}

#[test]
fn dependency_needs_upgrade() {
    TestScenario::new()
        .install("bar")
        .packages([
            package!("foo" = "1"),
            package!("foo" = "2", installed = true),
            package!("foo" = "3").must(Install),
            package!("bar" = "0", deps = ["foo (>> 2)"]).must(Install),
        ])
        .check_solution();
}

#[test]
fn remove() {
    TestScenario::new()
        .remove("foo")
        .packages([
            package!("qux" = "0", installed = true, deps = ["baz"]).must(Remove),
            package!("baz" = "0", installed = true, deps = ["foo", "bar"]).must(Remove),
            package!("bar" = "0", installed = true, deps = ["foo (>= 2)"]).must(Remove),
            package!("foo" = "1"),
            package!("foo" = "2", installed = true).must(Remove),
            package!("foo" = "3"),
            package!("quux" = "0", installed = true),
        ])
        .check_solution();
}
