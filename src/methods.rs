//! Execution plans for composed method bodies. The central body is virtual:
//! subtype definitions replace it while ancestor before/after wrappers remain.
//! Compilation uses an arena and explicit work stack rather than recursive
//! traversal, including for deeply extended methods.

use crate::{
    ast::{MethodDecl, MethodModifier},
    elab::ElaboratedType,
    interp::Value,
};
use std::{collections::BTreeMap, ops::Range};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BodyKind {
    Core,
    Before,
    After,
}

#[derive(Clone, Copy)]
pub(crate) enum Step<'a> {
    SaveArgs,
    RestoreArgs,
    Empty,
    Body {
        method: &'a MethodDecl,
        kind: BodyKind,
        on_return: usize,
    },
}

pub(crate) struct Program<'a> {
    pub owner: Option<&'a str>,
    pub prototype: &'a MethodDecl,
    pub steps: Vec<Step<'a>>,
}

#[derive(Debug, Clone)]
pub(crate) struct Progress {
    pub args: Vec<Value>,
    pub saved_args: Vec<Vec<Value>>,
    pub result: Value,
    pub pc: usize,
}

impl Progress {
    pub fn new(args: Vec<Value>) -> Self {
        Self {
            args,
            saved_args: Vec::new(),
            result: Value::Void,
            pc: 0,
        }
    }
    pub fn control(&mut self, step: Step<'_>) -> bool {
        match step {
            Step::SaveArgs => self.saved_args.push(self.args.clone()),
            Step::RestoreArgs => {
                self.args = self.saved_args.pop().expect("balanced method arguments")
            }
            _ => return false,
        }
        self.pc += 1;
        true
    }
}

enum Node<'a> {
    Core(Option<&'a MethodDecl>),
    Before(&'a MethodDecl, usize),
    After(usize, &'a MethodDecl),
    Alias(usize),
}

impl<'a> Program<'a> {
    pub fn instance(
        types: &'a [ElaboratedType],
        layers: &BTreeMap<String, Vec<Range<usize>>>,
        owner: &str,
        name: &str,
    ) -> Self {
        let ty = types
            .iter()
            .find(|ty| ty.name == owner)
            .expect("resolved method owner");
        let fallback = 0..ty.methods.len();
        let ranges = layers
            .get(owner)
            .map_or(std::slice::from_ref(&fallback), Vec::as_slice);
        let mut program = Self::compile(ranges.iter().map(|range| {
            ty.methods[range.clone()]
                .iter()
                .filter(|method| method.name == name)
                .collect()
        }));
        program.owner = Some(&ty.name);
        program
    }

    pub fn static_method(blocks: &[&'a MethodDecl]) -> Self {
        Self::compile(std::iter::once(blocks.to_vec()))
    }

    fn compile(layers: impl IntoIterator<Item = Vec<&'a MethodDecl>>) -> Self {
        let mut nodes = vec![Node::Core(None)];
        let mut core = 0;
        let mut blocks = Vec::new();
        for layer in layers {
            if layer.is_empty() {
                continue;
            }
            let Node::Core(old_body) = nodes[core] else {
                unreachable!("virtual method core");
            };
            let mut selected = nodes.len();
            nodes.push(Node::Core(old_body));
            let mut local = selected;
            for method in layer {
                blocks.push(method);
                let next = nodes.len();
                match method.modifier {
                    None | Some(MethodModifier::Only) => {
                        nodes.push(Node::Core(Some(method)));
                        selected = next;
                    }
                    Some(MethodModifier::First) => nodes.push(Node::Before(method, local)),
                    Some(MethodModifier::Also) => nodes.push(Node::After(local, method)),
                }
                local = next;
            }
            nodes[core] = Node::Alias(local);
            core = selected;
        }
        let prototype = blocks
            .iter()
            .copied()
            .find(|method| method.modifier.is_none())
            .or_else(|| blocks.first().copied())
            .expect("resolved method has a block");
        enum Task<'a> {
            Visit(usize),
            EndBefore(usize),
            After(&'a MethodDecl),
        }
        let mut tasks = vec![Task::Visit(0)];
        let mut steps = Vec::new();
        while let Some(task) = tasks.pop() {
            match task {
                Task::Visit(index) => match nodes[index] {
                    Node::Alias(next) => tasks.push(Task::Visit(next)),
                    Node::Core(None) => steps.push(Step::Empty),
                    Node::Core(Some(method)) => steps.push(Step::Body {
                        method,
                        kind: BodyKind::Core,
                        on_return: steps.len() + 1,
                    }),
                    Node::Before(method, prior) => {
                        steps.push(Step::SaveArgs);
                        let body = steps.len();
                        steps.push(Step::Body {
                            method,
                            kind: BodyKind::Before,
                            on_return: 0,
                        });
                        tasks.push(Task::EndBefore(body));
                        tasks.push(Task::Visit(prior));
                    }
                    Node::After(prior, method) => {
                        steps.push(Step::SaveArgs);
                        tasks.push(Task::After(method));
                        tasks.push(Task::Visit(prior));
                    }
                },
                Task::EndBefore(body) => {
                    let target = steps.len();
                    if let Step::Body { on_return, .. } = &mut steps[body] {
                        *on_return = target;
                    }
                    steps.push(Step::RestoreArgs);
                }
                Task::After(method) => {
                    steps.push(Step::RestoreArgs);
                    steps.push(Step::Body {
                        method,
                        kind: BodyKind::After,
                        on_return: steps.len() + 1,
                    });
                }
            }
        }
        Self {
            owner: None,
            prototype,
            steps,
        }
    }
}
