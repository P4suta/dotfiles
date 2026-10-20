//! The signing agent is independent of the session's push agent.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Socket {
    Present,
    Missing,
    NotSocket,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Route {
    PasswordManager,
    RefuseMissing,
    RefuseNotSocket,
}

#[must_use]
pub const fn route(signing: bool, socket: Socket) -> Route {
    match (signing, socket) {
        (true, Socket::Missing) => Route::RefuseMissing,
        (true, Socket::NotSocket) => Route::RefuseNotSocket,
        (false, _) | (true, Socket::Present) => Route::PasswordManager,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SocketPath {
    Absolute,
    HomeRelative,
    Invalid,
}

#[must_use]
pub const fn socket_path(empty: bool, absolute: bool, home_prefix: bool) -> SocketPath {
    if empty {
        SocketPath::Invalid
    } else if absolute {
        SocketPath::Absolute
    } else if home_prefix {
        SocketPath::HomeRelative
    } else {
        SocketPath::Invalid
    }
}

#[cfg(kani)]
mod proofs {
    use super::*;

    #[kani::proof]
    fn only_signing_requires_a_present_password_manager_socket() {
        let signing: bool = kani::any();
        let socket: Socket = kani::any();
        let result = route(signing, socket);
        assert_eq!(
            result == Route::PasswordManager,
            !signing || socket == Socket::Present
        );
        assert_eq!(
            result == Route::RefuseMissing,
            signing && socket == Socket::Missing
        );
        assert_eq!(
            result == Route::RefuseNotSocket,
            signing && socket == Socket::NotSocket
        );
        kani::cover!(result == Route::PasswordManager && signing);
        kani::cover!(result == Route::PasswordManager && !signing);
        kani::cover!(result == Route::RefuseMissing);
        kani::cover!(result == Route::RefuseNotSocket);
    }

    #[kani::proof]
    fn socket_paths_never_resolve_from_the_working_directory() {
        let empty: bool = kani::any();
        let absolute: bool = kani::any();
        let home_prefix: bool = kani::any();
        let result = socket_path(empty, absolute, home_prefix);
        assert_eq!(
            result == SocketPath::Invalid,
            empty || !(absolute || home_prefix)
        );
        assert_eq!(result == SocketPath::Absolute, !empty && absolute);
        assert_eq!(
            result == SocketPath::HomeRelative,
            !empty && !absolute && home_prefix
        );
        kani::cover!(result == SocketPath::Invalid);
        kani::cover!(result == SocketPath::Absolute);
        kani::cover!(result == SocketPath::HomeRelative);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_requires_the_password_manager_and_verification_needs_no_agent() {
        assert_eq!(route(true, Socket::Missing), Route::RefuseMissing);
        assert_eq!(route(true, Socket::NotSocket), Route::RefuseNotSocket);
        assert_eq!(route(true, Socket::Present), Route::PasswordManager);
        for socket in [Socket::Present, Socket::Missing, Socket::NotSocket] {
            assert_eq!(route(false, socket), Route::PasswordManager);
        }
    }

    #[test]
    fn paths_require_an_absolute_path_or_an_explicit_home_prefix() {
        assert_eq!(socket_path(true, false, false), SocketPath::Invalid);
        assert_eq!(socket_path(false, false, false), SocketPath::Invalid);
        assert_eq!(socket_path(false, false, true), SocketPath::HomeRelative);
        assert_eq!(socket_path(false, true, false), SocketPath::Absolute);
    }
}
