use std::str::FromStr;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum State {
    Clean,       // not visible
    Modified,    // M
    Added,       // A
    Deleted,     // D
    Unversioned, // ?
    Conflicting, // C
    Replaced,    // R
    Missing,     // !
}

impl State {
    pub(crate) fn is_commitable(&self) -> bool {
        match self {
            State::Modified | State::Added | State::Deleted => true,
            _ => false,
        }
    }

    pub(crate) fn is_revertable(&self) -> bool {
        match self {
            State::Modified
            | State::Added
            | State::Deleted
            | State::Conflicting
            | State::Missing => true,
            _ => false,
        }
    }

    pub(crate) fn is_deletable(&self) -> bool {
        match self {
            State::Modified | State::Missing | State::Conflicting => true,
            _ => false,
        }
    }
}

impl FromStr for State {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.chars().nth(0) {
            // TODO maybe use bitflags instead of an enum if we want to cover the other possibilities??
            Some(ch) => match ch {
                'M' => Ok(State::Modified),
                'A' => Ok(State::Added),
                'D' => Ok(State::Deleted),
                '?' => Ok(State::Unversioned),
                'C' => Ok(State::Conflicting),
                'R' => Ok(State::Replaced),
                '!' => Ok(State::Missing),
                _ => Ok(State::Clean), // TODO not sure if this is a good approach, because this might mean something else is wrong with the file/path
            },
            None => Err(()),
        }
    }
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                State::Clean => " ",
                State::Modified => "M",
                State::Added => "A",
                State::Deleted => "D",
                State::Unversioned => "?",
                State::Conflicting => "C",
                State::Replaced => "R",
                State::Missing => "!",
            }
        )
    }
}
