//! Stack-bounded ownership and value operations for the lossless source tree.
use super::{
    WriteError,
    positions::{SourceKind, SourceValue, YamlKind, YamlNode, unsupported},
};
use serde_json::Value;

pub(super) trait TreeShape: Sized {
    fn children(&self) -> usize;
    fn child(&self, index: usize) -> &Self;
}
impl TreeShape for YamlNode {
    fn children(&self) -> usize {
        match &self.kind {
            YamlKind::Sequence(items) => items.len(),
            YamlKind::Mapping(items) => items.len() * 2,
            _ => 0,
        }
    }
    fn child(&self, index: usize) -> &Self {
        match &self.kind {
            YamlKind::Sequence(items) => &items[index],
            YamlKind::Mapping(items) => {
                let pair = &items[index / 2];
                if index.is_multiple_of(2) {
                    &pair.0
                } else {
                    &pair.1
                }
            }
            _ => unreachable!("leaf has no children"),
        }
    }
}
impl TreeShape for SourceValue {
    fn children(&self) -> usize {
        match &self.value {
            SourceKind::Sequence(items) => items.len(),
            SourceKind::Mapping(items) => items.len() * 2,
            _ => 0,
        }
    }
    fn child(&self, index: usize) -> &Self {
        match &self.value {
            SourceKind::Sequence(items) => &items[index],
            SourceKind::Mapping(items) => {
                let pair = &items[index / 2];
                if index.is_multiple_of(2) {
                    &pair.0
                } else {
                    &pair.1
                }
            }
            _ => unreachable!("leaf has no children"),
        }
    }
}
struct CopyFrame<'a, T, U> {
    source: &'a T,
    output: U,
    next: usize,
    key: Option<U>,
}
pub(super) fn copy_tree<T: TreeShape, U>(
    root: &T,
    shell: fn(&T) -> U,
    attach: fn(&mut U, &mut Option<U>, U),
) -> U {
    let mut frames = vec![CopyFrame {
        source: root,
        output: shell(root),
        next: 0,
        key: None,
    }];
    loop {
        let frame = frames.last_mut().expect("copy has a root frame");
        if frame.next < frame.source.children() {
            let child = frame.source.child(frame.next);
            frame.next += 1;
            frames.push(CopyFrame {
                source: child,
                output: shell(child),
                next: 0,
                key: None,
            });
        } else {
            let complete = frames.pop().expect("completed copy frame");
            if let Some(parent) = frames.last_mut() {
                attach(&mut parent.output, &mut parent.key, complete.output);
            } else {
                return complete.output;
            }
        }
    }
}
pub(super) fn attach_yaml(parent: &mut YamlNode, key: &mut Option<YamlNode>, child: YamlNode) {
    match &mut parent.kind {
        YamlKind::Sequence(items) => items.push(child),
        YamlKind::Mapping(items) => {
            if let Some(key) = key.take() {
                items.push((key, child));
            } else {
                *key = Some(child);
            }
        }
        _ => unreachable!("only collection frames receive children"),
    }
}
pub(super) fn attach_source(
    parent: &mut SourceValue,
    key: &mut Option<SourceValue>,
    child: SourceValue,
) {
    match &mut parent.value {
        SourceKind::Sequence(items) => items.push(child),
        SourceKind::Mapping(items) => {
            if let Some(key) = key.take() {
                items.push((key, child));
            } else {
                *key = Some(child);
            }
        }
        _ => unreachable!("only collection frames receive children"),
    }
}
fn yaml_shell(node: &YamlNode) -> YamlNode {
    let kind = match &node.kind {
        YamlKind::Scalar { value, plain } => YamlKind::Scalar {
            value: value.clone(),
            plain: *plain,
        },
        YamlKind::Alias(id) => YamlKind::Alias(*id),
        YamlKind::Sequence(items) => YamlKind::Sequence(Vec::with_capacity(items.len())),
        YamlKind::Mapping(items) => YamlKind::Mapping(Vec::with_capacity(items.len())),
    };
    YamlNode {
        kind,
        start: node.start,
        end: node.end,
        flow: node.flow,
        anchor: node.anchor,
        tag: node.tag.clone(),
        style: node.style,
    }
}
pub(super) fn semantic_shell(node: &YamlNode) -> SourceValue {
    let value = match &node.kind {
        YamlKind::Scalar { value, plain } => SourceKind::Scalar(value.clone(), *plain, node.style),
        YamlKind::Alias(id) => SourceKind::Alias(*id),
        YamlKind::Sequence(items) => SourceKind::Sequence(Vec::with_capacity(items.len())),
        YamlKind::Mapping(items) => SourceKind::Mapping(Vec::with_capacity(items.len())),
    };
    SourceValue {
        value,
        anchor: node.anchor,
        tag: node.tag.clone(),
    }
}
fn source_shell(node: &SourceValue) -> SourceValue {
    let value = match &node.value {
        SourceKind::Scalar(value, plain, style) => {
            SourceKind::Scalar(value.clone(), *plain, *style)
        }
        SourceKind::Alias(id) => SourceKind::Alias(*id),
        SourceKind::Sequence(items) => SourceKind::Sequence(Vec::with_capacity(items.len())),
        SourceKind::Mapping(items) => SourceKind::Mapping(Vec::with_capacity(items.len())),
    };
    SourceValue {
        value,
        anchor: node.anchor,
        tag: node.tag.clone(),
    }
}
pub(super) fn equal_trees<T: TreeShape>(left: &T, right: &T, equal: fn(&T, &T) -> bool) -> bool {
    let mut pending = vec![(left, right)];
    while let Some((left, right)) = pending.pop() {
        if !equal(left, right) {
            return false;
        }
        for index in (0..left.children()).rev() {
            pending.push((left.child(index), right.child(index)));
        }
    }
    true
}
pub(super) fn semantic_node_equal(left: &YamlNode, right: &YamlNode) -> bool {
    if left.anchor != right.anchor || left.tag != right.tag || left.style != right.style {
        return false;
    }
    match (&left.kind, &right.kind) {
        (
            YamlKind::Scalar {
                value: a,
                plain: ap,
            },
            YamlKind::Scalar {
                value: b,
                plain: bp,
            },
        ) => a == b && ap == bp,
        (YamlKind::Alias(a), YamlKind::Alias(b)) => a == b,
        (YamlKind::Sequence(a), YamlKind::Sequence(b)) => a.len() == b.len(),
        (YamlKind::Mapping(a), YamlKind::Mapping(b)) => a.len() == b.len(),
        _ => false,
    }
}
fn source_equal(left: &SourceValue, right: &SourceValue) -> bool {
    if left.anchor != right.anchor || left.tag != right.tag {
        return false;
    }
    match (&left.value, &right.value) {
        (SourceKind::Scalar(a, ap, ast), SourceKind::Scalar(b, bp, bst)) => {
            a == b && ap == bp && ast == bst
        }
        (SourceKind::Alias(a), SourceKind::Alias(b)) => a == b,
        (SourceKind::Sequence(a), SourceKind::Sequence(b)) => a.len() == b.len(),
        (SourceKind::Mapping(a), SourceKind::Mapping(b)) => a.len() == b.len(),
        _ => false,
    }
}
impl Clone for YamlNode {
    fn clone(&self) -> Self {
        copy_tree(self, yaml_shell, attach_yaml)
    }
}
impl PartialEq for YamlNode {
    fn eq(&self, other: &Self) -> bool {
        equal_trees(self, other, |a, b| {
            a.start == b.start && a.end == b.end && a.flow == b.flow && semantic_node_equal(a, b)
        })
    }
}
impl Eq for YamlNode {}
impl Clone for SourceValue {
    fn clone(&self) -> Self {
        copy_tree(self, source_shell, attach_source)
    }
}
impl PartialEq for SourceValue {
    fn eq(&self, other: &Self) -> bool {
        equal_trees(self, other, source_equal)
    }
}
impl Eq for SourceValue {}
impl Drop for YamlNode {
    fn drop(&mut self) {
        if self.children() == 0 {
            return;
        }
        let mut pending = Vec::new();
        let mut current = Some(std::mem::replace(&mut self.kind, YamlKind::Alias(0)));
        while let Some(kind) = current {
            match kind {
                YamlKind::Sequence(items) => {
                    for mut node in items {
                        if node.children() != 0 {
                            pending.push(std::mem::replace(&mut node.kind, YamlKind::Alias(0)));
                        }
                    }
                }
                YamlKind::Mapping(items) => {
                    for (mut key, mut value) in items {
                        if key.children() != 0 {
                            pending.push(std::mem::replace(&mut key.kind, YamlKind::Alias(0)));
                        }
                        if value.children() != 0 {
                            pending.push(std::mem::replace(&mut value.kind, YamlKind::Alias(0)));
                        }
                    }
                }
                _ => {}
            }
            current = pending.pop();
        }
    }
}
impl Drop for SourceValue {
    fn drop(&mut self) {
        if self.children() == 0 {
            return;
        }
        let mut pending = Vec::new();
        let mut current = Some(std::mem::replace(&mut self.value, SourceKind::Alias(0)));
        while let Some(kind) = current {
            match kind {
                SourceKind::Sequence(items) => {
                    for mut node in items {
                        if node.children() != 0 {
                            pending.push(std::mem::replace(&mut node.value, SourceKind::Alias(0)));
                        }
                    }
                }
                SourceKind::Mapping(items) => {
                    for (mut key, mut value) in items {
                        if key.children() != 0 {
                            pending.push(std::mem::replace(&mut key.value, SourceKind::Alias(0)));
                        }
                        if value.children() != 0 {
                            pending.push(std::mem::replace(&mut value.value, SourceKind::Alias(0)));
                        }
                    }
                }
                _ => {}
            }
            current = pending.pop();
        }
    }
}
struct JsonFrame<'a> {
    source: &'a YamlNode,
    output: Value,
    next: usize,
    key: Option<String>,
}
fn json_shell(node: &YamlNode) -> Result<Value, WriteError> {
    node.editable()?;
    Ok(match &node.kind {
        YamlKind::Scalar { value, plain } => {
            if *plain && matches!(value.as_str(), "null" | "Null" | "NULL" | "~" | "") {
                Value::Null
            } else if *plain && matches!(value.as_str(), "true" | "false") {
                Value::Bool(value == "true")
            } else if let Some(number) = plain
                .then(|| value.parse::<serde_json::Number>().ok())
                .flatten()
            {
                Value::Number(number)
            } else {
                Value::String(value.clone())
            }
        }
        YamlKind::Sequence(items) => Value::Array(Vec::with_capacity(items.len())),
        YamlKind::Mapping(_) => Value::Object(serde_json::Map::new()),
        YamlKind::Alias(_) => return Err(unsupported("YAML alias target")),
    })
}
pub(super) fn json_tree(root: &YamlNode) -> Result<Value, WriteError> {
    let mut frames = vec![JsonFrame {
        source: root,
        output: json_shell(root)?,
        next: 0,
        key: None,
    }];
    loop {
        let frame = frames.last_mut().expect("JSON conversion has a root frame");
        if frame.next < frame.source.children() {
            let child = frame.source.child(frame.next);
            if matches!(frame.source.kind, YamlKind::Mapping(_)) && frame.next.is_multiple_of(2) {
                child.editable()?;
                let key = child
                    .scalar()
                    .ok_or_else(|| unsupported("complex YAML keys are unsupported targets"))?;
                if frame
                    .output
                    .as_object()
                    .expect("mapping JSON frame")
                    .contains_key(key)
                {
                    return Err(unsupported(format!("duplicate YAML key `{key}`")));
                }
                frame.key = Some(key.into());
                frame.next += 1;
                continue;
            }
            frame.next += 1;
            if matches!(child.kind, YamlKind::Sequence(_) | YamlKind::Mapping(_))
                && frames.len() >= 64
            {
                return Err(unsupported(
                    "whole JSON conversion exceeds the 64-level metadata bound; use borrowed source spans for a deep exploration tree",
                ));
            }
            frames.push(JsonFrame {
                source: child,
                output: json_shell(child)?,
                next: 0,
                key: None,
            });
        } else {
            let complete = frames.pop().expect("completed JSON frame");
            if let Some(parent) = frames.last_mut() {
                match &mut parent.output {
                    Value::Array(items) => items.push(complete.output),
                    Value::Object(items) => {
                        items.insert(
                            parent.key.take().expect("mapping key precedes value"),
                            complete.output,
                        );
                    }
                    _ => unreachable!("JSON leaf has no children"),
                }
            } else {
                return Ok(complete.output);
            }
        }
    }
}
