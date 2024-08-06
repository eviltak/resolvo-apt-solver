use apt_edsp::scenario::Scenario;
use apt_edsp::Bool;

use super::*;

const ARCH: &'static str = "amd64";

macro_rules! scenario {
    ($(install = $install:literal,)?
    $(remove = $remove:literal,)?
    packages = [$($package:expr),* $(,)?]) => {
        apt_edsp::scenario::Scenario {
            request: apt_edsp::scenario::Request {
                request: "EDSP 0.5".into(),
                architecture: ARCH.into(),
                actions: apt_edsp::scenario::Actions {
                    $(install: Some(From::from($install)),)?
                    $(remove: Some(From::from($remove)),)?
                    ..Default::default()
                },
                ..Default::default()
            },
            universe: vec![$($package),*],
        }
    };
}

macro_rules! package {
    ($name:literal $(: $arch:literal)? = $version:literal $(, installed = $installed:literal)? $(, deps = [$($dep:literal),*])? $(, conflicts = [$($conflict:literal),*])?) => {
        apt_edsp::scenario::Package {
            package: $name.into(),
            version: $version.try_into().unwrap(),
            architecture: Some(ARCH.into()).map(#[allow(unreachable_code)] |a| { $(return $arch.into();)? a }).unwrap(),
            installed: None.or_else(#[allow(unreachable_code)] || { $(return Some($installed.into());)? None }).unwrap_or(Bool::default()),
            $(depends: vec![$($dep.try_into().unwrap()),*],)?
            $(conflicts: vec![$($conflict.try_into().unwrap()),*],)?
            ..Default::default()
        }
    };
}

fn solve_snapshot(scenario: &Scenario) -> Answer {
    let _ = tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .finish(),
    );
    solve(&scenario)
}

#[test]
fn simple() {
    let scenario = scenario! {
        install = "baz", packages = [
            package!("foo"="1"),
            package!("foo"="2"),
            package!("foo"="3"),
            package!("bar"="0", conflicts = ["foo (>= 2)"]),
            package!("baz"="0", deps = ["foo", "bar"]),
        ]
    };

    println!("{:?}", solve_snapshot(&scenario));
}

#[test]
fn request_conflicts_with_installed() {
    let scenario = scenario! {
        install = "bar", packages = [
            package!("foo"="0", conflicts = ["qux"]),
            package!("foo"="1"),
            package!("foo"="2", conflicts = ["qux"]),
            package!("foo"="3", installed = true),
            package!("bar"="0", conflicts = ["foo (>= 2)"]),
            package!("qux"="0", installed = true),
        ]
    };

    println!("{:?}", solve_snapshot(&scenario));
}

#[test]
fn installed_conflicts_with_request() {
    let scenario = scenario! {
        install = "bar", packages = [
            package!("foo"="0", conflicts = ["bar"]),
            package!("foo"="1"),
            package!("foo"="2", conflicts = ["bar"]),
            package!("foo"="3", installed = true, conflicts = ["bar"]),
            package!("bar"="0"),
            package!("qux"="0", installed = true),
        ]
    };

    println!("{:?}", solve_snapshot(&scenario));
}

#[test]
fn old_dependency_installed() {
    let scenario = scenario! {
        install = "bar", packages = [
            package!("foo"="1"),
            package!("foo"="2", installed = true),
            package!("foo"="3"),
            package!("bar"="0", deps = ["foo"])
        ]
    };

    println!("{:?}", solve_snapshot(&scenario));
}

#[test]
fn installed_depends_on_older_version() {
    let scenario = scenario! {
        install = "baz", packages = [
            package!("foo"="1", installed = true),
            package!("foo"="2"),
            package!("bar"="0", installed = true, deps = ["foo (= 1)"]),
            package!("baz"="0", deps = ["foo (= 2)"]),
        ]
    };

    println!("{:?}", solve_snapshot(&scenario));
}

#[test]
fn dependency_needs_upgrade() {
    let scenario = scenario! {
        install = "bar", packages = [
            package!("foo"="1"),
            package!("foo"="2", installed = true),
            package!("foo"="3"),
            package!("bar"="0", deps = ["foo (>> 2)"])
        ]
    };

    println!("{:?}", solve_snapshot(&scenario));
}

#[test]
fn remove() {
    let scenario = scenario! {
        remove = "foo", packages = [
            package!("qux"="0", installed = true, deps = ["baz"]),
            package!("baz"="0", installed = true, deps = ["foo", "bar"]),
            package!("bar"="0", installed = true, deps = ["foo (>= 2)"]),
            package!("foo"="1"),
            package!("foo"="2", installed = true),
            package!("foo"="3"),
            package!("quux"="0", installed = true),
        ]
    };

    println!("{:?}", solve_snapshot(&scenario));
}
