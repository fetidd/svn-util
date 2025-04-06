#![allow(dead_code, unused_variables)]
use std::path::PathBuf;

use super::state::State;

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub enum TreeNode {
    File { path: PathBuf, state: State },
    Dir { path: PathBuf, tree: Tree },
}

#[derive(Default, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Tree {
    nodes: Vec<TreeNode>,
}

impl Tree {
    pub fn build_from_svn_status(svn_status: &str) -> super::Result<Self> {
        let mut parsed = super::parse_svn_status(svn_status)?;
        let t = Self::default();
        for (i, (_, path)) in parsed.iter_mut().enumerate() {
            let components = path.components().collect::<Vec<_>>();
        }
        Ok(t)
    }
}

#[cfg(test)]
mod tests {
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
        let t = Tree::build_from_svn_status(svn_output).expect("failed to populate");
        let expected = Tree {
            nodes: vec![
                TreeNode::File {
                    path: "file1.txt".into(),
                    state: State::Modified,
                },
                TreeNode::Dir {
                    path: "dir1".into(),
                    tree: Tree {
                        nodes: vec![
                            TreeNode::File {
                                path: "file2.txt".into(),
                                state: State::Modified,
                            },
                            TreeNode::Dir {
                                path: "nested1".into(),
                                tree: Tree {
                                    nodes: vec![TreeNode::File {
                                        path: "file3.txt".into(),
                                        state: State::Modified,
                                    }],
                                },
                            },
                        ],
                    },
                },
                TreeNode::Dir {
                    path: "dir2".into(),
                    tree: Tree {
                        nodes: vec![
                            TreeNode::File {
                                path: "newfile.txt".into(),
                                state: State::Added,
                            },
                            TreeNode::File {
                                path: "newimage.png".into(),
                                state: State::Added,
                            },
                        ],
                    },
                },
            ],
        };
        // assert_eq!(t, expected);
    }
}
