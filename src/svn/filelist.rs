use std::{collections::HashMap, path::PathBuf};

use super::{
    Conflict, ConflictPart, ParsedStatusLine, create_empty_text_conflict, is_conflict_part,
    parse_conflict_part, parse_svn_status, state::State, trim_conflict_suffix,
};

#[derive(Debug, Clone, PartialEq)]
pub struct FileList {
    list: Vec<ParsedStatusLine>,
}

impl FileList {
    pub fn list(&self) -> &[ParsedStatusLine] {
        &self.list
    }

    pub fn list_mut(&mut self) -> &mut Vec<ParsedStatusLine> {
        &mut self.list
    }

    pub fn empty() -> Self {
        Self { list: vec![] }
    }

    pub fn populate_from_svn_status(&mut self, svn_status: &str) -> super::Result<()> {
        *self.list_mut() = parse_svn_status(svn_status)?;
        Ok(())
    }

    pub fn conflicts(&self) -> Vec<Conflict> {
        let mut conflict_map = HashMap::new();
        for (state, path) in self.list().iter() {
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
        self.list()
            .iter()
            .any(|(state, _)| *state == State::Conflicting)
    }

    pub fn get(&self, index: usize) -> Option<&(State, PathBuf)> {
        self.list()
            .iter()
            .filter(|(_, path)| !is_conflict_part(path.to_str().unwrap()))
            .nth(index)
    }

    pub fn renderable(&self) -> Vec<&ParsedStatusLine> {
        self.list()
            .iter()
            .filter(|(_, path)| !is_conflict_part(path.to_str().unwrap()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::State::*;
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn can_populate_from_svn_output() {
        let svn_output = "
M       file1.txt
M       dir1/file2.txt
M       dir1/nested1/file3.txt
A       dir2/newfile1.txt
A       dir2/newimage.png
 M      .
R       replaced.txt
!       missing.txt
?       new.txt
D       deleted.txt
C       conflict.txt
";
        let mut l = FileList::empty();
        l.populate_from_svn_status(svn_output)
            .expect("failed to populate");
        assert_eq!(true, l.has_conflicts());
        assert_eq!(
            l,
            FileList {
                list: vec![
                    (Modified, "file1.txt".into()),
                    (Modified, "dir1/file2.txt".into()),
                    (Modified, "dir1/nested1/file3.txt".into()),
                    (Added, "dir2/newfile1.txt".into()),
                    (Added, "dir2/newimage.png".into()),
                    (Clean, ".".into()),
                    (Replaced, "replaced.txt".into()),
                    (Missing, "missing.txt".into()),
                    (Unversioned, "new.txt".into()),
                    (Deleted, "deleted.txt".into()),
                    (Conflicting, "conflict.txt".into()),
                ]
            }
        )
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

    #[test]
    fn can_get_changes_correctly() {
        let svn_output = "C       dir1/file3.txt
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
        assert!(l.has_conflicts());
        assert_eq!(Some(&l.list()[5]), l.get(2)); // the get method skips the conflict parts
        assert_eq!(Some(&l.list()[0]), l.get(0));
    }
}
