use std::collections::HashMap;

use super::{
    Conflict, ConflictPart, ParsedStatusLine, create_empty_text_conflict, is_conflict_part,
    parse_conflict_part, parse_svn_status, state::State, trim_conflict_suffix,
};

#[derive(Debug, Clone)]
pub struct FileList(Vec<ParsedStatusLine>);

impl FileList {
    pub fn list(&self) -> &[ParsedStatusLine] {
        &self.0
    }

    pub fn empty() -> Self {
        Self(vec![])
    }

    pub fn populate_from_svn_status(&mut self, svn_status: &str) -> Result<(), ()> {
        self.0 = parse_svn_status(svn_status)?;
        Ok(())
    }

    pub fn conflicts(&self) -> Vec<Conflict> {
        let mut conflict_map = HashMap::new();
        for (state, path) in self.0.iter() {
            let path_str = &path.to_str().expect("bad path");
            if *state == State::Conflicting && !conflict_map.contains_key(path_str) {
                conflict_map.insert(*path_str, create_empty_text_conflict(path));
            } else if *state == State::Unversioned && is_conflict_part(path_str) {
                let path_key = trim_conflict_suffix(path_str);
                conflict_map
                    .entry(path_key)
                    .and_modify(|conflict| match conflict {
                        Conflict::Text {
                            left,
                            right,
                            working,
                            ..
                        } => {
                            if let Some(part) = parse_conflict_part(path_str) {
                                let prop = match part {
                                    ConflictPart::Left => left,
                                    ConflictPart::Right => right,
                                    ConflictPart::Working => working,
                                };
                                *prop = Some(path.clone());
                            } else {
                                panic!("do this instead of is_cnflictpart?>");
                            }
                        }
                    })
                    .or_insert(create_empty_text_conflict(path));
            }
        }
        conflict_map.into_values().collect()
    }

    pub fn has_conflicts(&self) -> bool {
        self.0.iter().any(|(state, _)| *state == State::Conflicting)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn can_populate_from_svn_output() {
        let svn_output = "
M       file1.txt
M       dir1/file2.txt
M       dir1/nested1/file3.txt
A       dir2/newfile1.txt
A       dir2/newimage.png
";
        let mut l = FileList::empty();
        l.populate_from_svn_status(svn_output)
            .expect("failed to populate");
        assert_eq!(false, l.has_conflicts());
    }

    #[test]
    fn parses_text_conflicts_correctly() {
        let svn_output = "
C       dir1/file3.txt
?       dir1/file3.txt.merge-left.r8
?       dir1/file3.txt.merge-right.r10
?       dir1/file3.txt.working
A  +    dir1/newfile.txt
M       dir2/nested1/file5.txt
D       file2.txt
A  +    newfile.txt
Summary of conflicts:
  Text conflicts: 1
";
        let mut l = FileList::empty();
        l.populate_from_svn_status(svn_output)
            .expect("failed to populate");
        assert_eq!(true, l.has_conflicts());
        assert_eq!(
            vec![Conflict::Text {
                file: PathBuf::from("dir1/file3.txt"),
                left: Some(PathBuf::from("dir1/file3.txt.merge-left.r8")),
                right: Some(PathBuf::from("dir1/file3.txt.merge-right.r10")),
                working: Some(PathBuf::from("dir1/file3.txt.working"))
            }],
            l.conflicts()
        );
    }
}
