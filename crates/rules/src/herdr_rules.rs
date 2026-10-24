//! SSH routing and Herdr recovery decisions consume observations without performing effects.

extern crate alloc;

pub const SSH_INCLUDE: &str = "Include ~/.config/dotfiles/ssh-agent.conf";

#[must_use]
pub fn ssh_config_preserved(current: &str, rendered: &str) -> bool {
    let host_line = |line: &&str| !line.trim().is_empty() && line.trim() != SSH_INCLUDE;
    rendered
        .lines()
        .filter(|line| line.trim() == SSH_INCLUDE)
        .count()
        == 1
        && current
            .lines()
            .filter(host_line)
            .eq(rendered.lines().filter(host_line))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Agent {
    Loaded,
    Empty,
    Missing,
    Stale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Context {
    Local,
    Explicit,
    Inherited,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Route {
    Herdr,
    PasswordManager,
    Explicit,
    Inherited,
}

#[must_use]
pub const fn route(context: Context, agent: Agent) -> Route {
    match context {
        Context::Explicit => Route::Explicit,
        Context::Inherited => Route::Inherited,
        Context::Local => match agent {
            Agent::Empty | Agent::Missing | Agent::Stale => Route::PasswordManager,
            Agent::Loaded => Route::Herdr,
        },
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Instances {
    pub servers: u16,
    pub abandoned: u16,
    pub responding: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Launch {
    Start,
    Attach,
    RefuseDuplicate,
    RefuseStale,
}

#[must_use]
pub const fn launch(instances: Instances) -> Launch {
    if instances.servers > 1 {
        Launch::RefuseDuplicate
    } else if instances.abandoned > 0 || (instances.servers == 1 && !instances.responding) {
        Launch::RefuseStale
    } else if instances.responding {
        Launch::Attach
    } else {
        Launch::Start
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Health {
    pub agent: Agent,
    pub instances: Launch,
    pub dead_daemon_socket: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Findings {
    pub agent: Option<Agent>,
    pub instances: Option<Launch>,
    pub dead_daemon_socket: bool,
}

#[must_use]
pub const fn health(facts: Health) -> Findings {
    Findings {
        agent: match facts.agent {
            Agent::Loaded => None,
            other => Some(other),
        },
        instances: match facts.instances {
            Launch::RefuseDuplicate | Launch::RefuseStale => Some(facts.instances),
            Launch::Start | Launch::Attach => None,
        },
        dead_daemon_socket: facts.dead_daemon_socket,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ensure {
    Ready,
    Load,
    StartService,
}

#[must_use]
pub const fn ensure(agent: Agent) -> Ensure {
    match agent {
        Agent::Loaded => Ensure::Ready,
        Agent::Empty => Ensure::Load,
        Agent::Missing | Agent::Stale => Ensure::StartService,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cleanup {
    Refuse,
    Terminate,
}

#[must_use]
pub const fn cleanup(confirmed: bool, same_instance: bool, owned_herdr: bool) -> Cleanup {
    if confirmed && same_instance && owned_herdr {
        Cleanup::Terminate
    } else {
        Cleanup::Refuse
    }
}

#[cfg(kani)]
mod proofs {
    use super::*;

    #[kani::proof]
    #[kani::unwind(128)]
    fn ssh_merge_preserves_bounded_host_content_and_rejects_deletion() {
        let bytes: [u8; 8] = kani::any();
        let Ok(host) = core::str::from_utf8(&bytes) else {
            return;
        };
        let rendered = alloc::format!("{SSH_INCLUDE}\n{host}");
        assert!(ssh_config_preserved(host, &rendered));
        assert_eq!(
            ssh_config_preserved(host, SSH_INCLUDE),
            host.trim().is_empty()
        );
        assert!(!ssh_config_preserved(
            host,
            &alloc::format!("{SSH_INCLUDE}\n{rendered}")
        ));
        kani::cover!(!host.trim().is_empty());
        kani::cover!(host.trim().is_empty());
    }

    #[kani::proof]
    fn only_a_loaded_local_agent_receives_ssh() {
        let context: Context = kani::any();
        let agent: Agent = kani::any();
        let result = route(context, agent);
        assert_eq!(
            result == Route::Herdr,
            context == Context::Local && agent == Agent::Loaded
        );
        assert_eq!(result == Route::Inherited, context == Context::Inherited);
        assert_eq!(result == Route::Explicit, context == Context::Explicit);
        assert_eq!(
            result == Route::PasswordManager,
            context == Context::Local && agent != Agent::Loaded
        );
        kani::cover!(result == Route::Herdr);
        kani::cover!(result == Route::PasswordManager);
        kani::cover!(result == Route::Inherited);
        kani::cover!(result == Route::Explicit);
    }

    #[kani::proof]
    fn stale_or_duplicate_processes_never_start_another_server() {
        let instances: Instances = kani::any();
        let result = launch(instances);
        assert_eq!(result == Launch::RefuseDuplicate, instances.servers > 1);
        assert_eq!(
            result == Launch::RefuseStale,
            instances.servers <= 1
                && (instances.abandoned > 0 || (instances.servers == 1 && !instances.responding))
        );
        if result == Launch::Start {
            assert_eq!(instances.servers, 0);
            assert_eq!(instances.abandoned, 0);
            assert!(!instances.responding);
        }
        kani::cover!(result == Launch::Start);
        kani::cover!(result == Launch::Attach);
        kani::cover!(result == Launch::RefuseDuplicate);
        kani::cover!(result == Launch::RefuseStale);
    }

    #[kani::proof]
    fn health_preserves_every_observed_failure() {
        let agent: Agent = kani::any();
        let instances = launch(kani::any());
        let dead_daemon_socket: bool = kani::any();
        let result = health(Health {
            agent,
            instances,
            dead_daemon_socket,
        });
        assert_eq!(result.agent.is_some(), agent != Agent::Loaded);
        assert_eq!(
            result.instances.is_some(),
            matches!(instances, Launch::RefuseDuplicate | Launch::RefuseStale)
        );
        assert_eq!(result.dead_daemon_socket, dead_daemon_socket);
        kani::cover!(result.agent == Some(Agent::Empty));
        kani::cover!(result.agent == Some(Agent::Stale));
        kani::cover!(result.dead_daemon_socket);
    }

    #[kani::proof]
    fn ensure_reloads_without_starting_an_unmanaged_agent() {
        let agent: Agent = kani::any();
        let result = ensure(agent);
        assert_eq!(result == Ensure::Ready, agent == Agent::Loaded);
        assert_eq!(result == Ensure::Load, agent == Agent::Empty);
        assert_eq!(
            result == Ensure::StartService,
            matches!(agent, Agent::Missing | Agent::Stale)
        );
        kani::cover!(result == Ensure::Load);
        kani::cover!(result == Ensure::StartService);
    }

    #[kani::proof]
    fn termination_requires_confirmation_and_the_same_owned_instance() {
        let confirmed: bool = kani::any();
        let same: bool = kani::any();
        let owned: bool = kani::any();
        assert_eq!(
            cleanup(confirmed, same, owned) == Cleanup::Terminate,
            confirmed && same && owned
        );
        kani::cover!(cleanup(confirmed, same, owned) == Cleanup::Terminate);
        kani::cover!(cleanup(confirmed, same, owned) == Cleanup::Refuse);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_merge_changes_only_the_single_managed_include() {
        let host = "Include ~/.orbstack/ssh/config\nHost local\n HostName fixture.invalid\n";
        let rendered = alloc::format!("{SSH_INCLUDE}\n{host}");
        assert!(ssh_config_preserved(host, &rendered));
        assert!(ssh_config_preserved(&rendered, &rendered));
        assert!(!ssh_config_preserved(
            host,
            &rendered.replace("fixture.invalid", "changed.invalid")
        ));
        assert!(!ssh_config_preserved(host, SSH_INCLUDE));
        assert!(!ssh_config_preserved(host, host));
    }

    #[test]
    fn routing_rechecks_loaded_empty_and_missing_agents() {
        assert_eq!(route(Context::Local, Agent::Loaded), Route::Herdr);
        assert_eq!(route(Context::Local, Agent::Empty), Route::PasswordManager);
        assert_eq!(
            route(Context::Local, Agent::Missing),
            Route::PasswordManager
        );
        assert_eq!(route(Context::Local, Agent::Stale), Route::PasswordManager);
        for agent in [Agent::Loaded, Agent::Empty, Agent::Missing, Agent::Stale] {
            assert_eq!(route(Context::Inherited, agent), Route::Inherited);
            assert_eq!(route(Context::Explicit, agent), Route::Explicit);
        }
    }

    #[test]
    fn duplicate_and_unresponsive_instances_never_start_another_server() {
        assert_eq!(
            launch(Instances {
                servers: 2,
                abandoned: 0,
                responding: true
            }),
            Launch::RefuseDuplicate
        );
        assert_eq!(
            launch(Instances {
                servers: 1,
                abandoned: 0,
                responding: false
            }),
            Launch::RefuseStale
        );
        assert_eq!(
            launch(Instances {
                servers: 0,
                abandoned: 1,
                responding: false
            }),
            Launch::RefuseStale
        );
        assert_eq!(
            launch(Instances {
                servers: 1,
                abandoned: 0,
                responding: true
            }),
            Launch::Attach
        );
        assert_eq!(
            launch(Instances {
                servers: 0,
                abandoned: 0,
                responding: false
            }),
            Launch::Start
        );
    }

    #[test]
    fn health_reports_empty_stale_duplicates_and_dead_daemon_sockets() {
        let facts = Health {
            agent: Agent::Empty,
            instances: Launch::RefuseDuplicate,
            dead_daemon_socket: true,
        };
        assert_eq!(
            health(facts),
            Findings {
                agent: Some(Agent::Empty),
                instances: Some(Launch::RefuseDuplicate),
                dead_daemon_socket: true
            }
        );
        assert_eq!(
            health(Health {
                agent: Agent::Stale,
                ..facts
            })
            .agent,
            Some(Agent::Stale)
        );
        assert_eq!(
            health(Health {
                agent: Agent::Loaded,
                instances: Launch::Attach,
                dead_daemon_socket: false
            }),
            Findings {
                agent: None,
                instances: None,
                dead_daemon_socket: false
            }
        );
    }

    #[test]
    fn cleanup_requires_all_three_facts_and_ensure_uses_the_service() {
        for confirmed in [false, true] {
            for same in [false, true] {
                for owned in [false, true] {
                    assert_eq!(
                        cleanup(confirmed, same, owned) == Cleanup::Terminate,
                        confirmed && same && owned
                    );
                }
            }
        }
        assert_eq!(ensure(Agent::Loaded), Ensure::Ready);
        assert_eq!(ensure(Agent::Empty), Ensure::Load);
        assert_eq!(ensure(Agent::Missing), Ensure::StartService);
        assert_eq!(ensure(Agent::Stale), Ensure::StartService);
    }
}
