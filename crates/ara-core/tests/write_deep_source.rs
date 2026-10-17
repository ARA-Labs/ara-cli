#![cfg(feature = "native")]
use ara_core::write::{
    WorkingArtifact, node,
    positions::{PathPart, YamlDocument},
    source::{ArtifactSnapshot, FileSnapshot, digest},
};
use serde_json::json;
use std::{collections::BTreeMap, fmt::Write as _, path::PathBuf};

fn nested_source(depth: usize) -> String {
    let mut source = String::from("tree: [");
    for id in 1..=depth {
        write!(
            &mut source,
            "{{id: N{id:05}, type: concept, title: É_你好, children: ["
        )
        .unwrap();
    }
    for _ in 0..depth {
        source.push_str("]}");
    }
    source.push_str("]\r\n");
    source
}

#[test]
fn true_ten_thousand_deep_native_source_index_clone_delta_and_drop_are_stack_bounded() {
    std::thread::Builder::new().stack_size(256*1024).spawn(||{
        const DEPTH:usize=10_000;
        let source = nested_source(DEPTH);
        let document=YamlDocument::parse(&source).unwrap();
        let mut current=&document.root.get("tree").unwrap().unwrap().sequence().unwrap()[0];
        for id in 1..=DEPTH{
            assert_eq!(current.get("id").unwrap().unwrap().scalar(),Some(format!("N{id:05}").as_str()));
            assert_eq!(current.get("title").unwrap().unwrap().scalar(),Some("É_你好"));
            let children=current.get("children").unwrap().unwrap().sequence().unwrap();
            if id==DEPTH{assert!(children.is_empty());}else{assert_eq!(children.len(),1);current=&children[0];}
        }
        let cloned=document.root.clone();assert!(document.root==cloned);
        let semantic=document.root.semantic();let cloned_semantic=semantic.clone();assert!(semantic==cloned_semantic);
        assert_eq!(YamlDocument::parse(&nested_source(DEPTH + 1)).unwrap_err().code, "write.unsupported_source");
        assert_eq!(YamlDocument::parse(&source[..source.len() - 3]).unwrap_err().code, "write.syntax");
        assert_eq!(document.root.to_json().unwrap_err().code,"write.unsupported_source");
        let files=BTreeMap::from([("trace/exploration_tree.yaml".into(),FileSnapshot{bytes:source.as_bytes().to_vec(),existed:true,permissions:None,digest:digest(source.as_bytes())})]);
        let mut working=WorkingArtifact::new(ArtifactSnapshot{root:PathBuf::from("/virtual/deep-source"),files,identity_paths:Default::default()});
        let expected:Vec<String>=(1..=DEPTH).map(|id|format!("N{id:05}")).collect();
        assert_eq!(node::node_ids(&working).unwrap(),expected);
        working.append_yaml("trace/exploration_tree.yaml",&[PathPart::from("tree")],&json!({"id":"N10001","type":"question","title":"New root","description":"Synthetic new root question"})).unwrap();
        assert!(working.text("trace/exploration_tree.yaml").unwrap().starts_with(&source[..source.len()-3]));
        let changed=working.yaml("trace/exploration_tree.yaml").unwrap();
        let roots=changed.root.get("tree").unwrap().unwrap().sequence().unwrap();
        assert_eq!(roots.len(),2);assert_eq!(roots[1].get("id").unwrap().unwrap().scalar(),Some("N10001"));
        assert!(document.root.get("tree").unwrap().unwrap().sequence().unwrap()[0].same_source_value(&roots[0]));
        // Every owned tree and the cache are destroyed on this small stack.
    }).unwrap().join().unwrap();
}
