//! Reverse-mode automatic differentiation on an arena of nodes.
//!
//! A calculation is written once, generically over [`Operand`], and runs on `f64` (no recording)
//! or on [`Var`] (recorded on a [`Recorder`]). A node's value is one number or a vector with one
//! number per path, so the same graph serves deterministic and Monte Carlo calculations.
//!
//! Nodes are pushed in the order they are computed, so the position in the arena is their
//! identifier and the arena is already in dependency order. There is no shared counter: each
//! calculation has its own recorder, and results do not depend on what other threads do.
//!
//! A `Recorder` is a local work area. It is mutated while recording through `RefCell`, which makes
//! it neither `Sync` nor shareable between threads by construction. Nothing else in the crate holds
//! interior mutability; see `docs/spec_autodiff.md` 3.2.
//!
//! Adjoints follow the pairing `d(output) = E[adjoint * d(node)]`, as in finmath-lib: an expected
//! value passes its adjoint on unchanged, and a number used on many paths receives the average of
//! the adjoints over the paths.
//!
//! Second derivatives come from [`Recorder::gradient_vars`], which records the backward pass itself
//! as new nodes, so that a derivative can be differentiated again.

use crate::math::normal::{normal_cdf, normal_pdf};
use crate::Error;
use rayon::prelude::*;
use std::cell::RefCell;
use std::ops::{Add, Div, Mul, Neg, Sub};
use std::sync::Arc;

/// The value of a node: one number, or one number per path.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Scalar(f64),
    Paths(Arc<[f64]>),
}

impl Value {
    /// The number of a scalar value.
    pub fn scalar(&self) -> Option<f64> {
        match self {
            Value::Scalar(x) => Some(*x),
            Value::Paths(_) => None,
        }
    }

    /// The average over the paths; a scalar is its own average.
    pub fn mean(&self) -> f64 {
        match self {
            Value::Scalar(x) => *x,
            Value::Paths(p) => mean(p),
        }
    }

    fn map(&self, f: impl Fn(f64) -> f64 + Send + Sync) -> Value {
        match self {
            Value::Scalar(x) => Value::Scalar(f(*x)),
            Value::Paths(p) => paths_from(p.len(), |i| f(p[i])),
        }
    }

    fn zip(&self, other: &Value, f: impl Fn(f64, f64) -> f64 + Send + Sync) -> Value {
        match (self, other) {
            (Value::Scalar(a), Value::Scalar(b)) => Value::Scalar(f(*a, *b)),
            (Value::Scalar(a), Value::Paths(b)) => paths_from(b.len(), |i| f(*a, b[i])),
            (Value::Paths(a), Value::Scalar(b)) => paths_from(a.len(), |i| f(a[i], *b)),
            (Value::Paths(a), Value::Paths(b)) => {
                assert_eq!(a.len(), b.len(), "path vectors of different lengths");
                paths_from(a.len(), |i| f(a[i], b[i]))
            }
        }
    }

    fn is_paths(&self) -> bool {
        matches!(self, Value::Paths(_))
    }
}

/// Below this length a vector operation stays on the calling thread. Each element is computed on its
/// own, so the result does not depend on the split.
const PARALLEL_LENGTH: usize = 4096;

fn paths_from(n: usize, f: impl Fn(usize) -> f64 + Send + Sync) -> Value {
    let values: Vec<f64> = if n < PARALLEL_LENGTH {
        (0..n).map(f).collect()
    } else {
        (0..n).into_par_iter().map(f).collect()
    };
    Value::Paths(values.into())
}

fn mean(paths: &[f64]) -> f64 {
    paths.iter().sum::<f64>() / paths.len() as f64
}

/// The calculations of the crate are generic over this trait. It is internal to the numerics
/// (principle R6): public functions take and return plain values.
pub trait Operand:
    Copy
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Neg<Output = Self>
    + Add<f64, Output = Self>
    + Sub<f64, Output = Self>
    + Mul<f64, Output = Self>
    + Div<f64, Output = Self>
{
    /// A constant in the same calculation as `self`.
    fn like(self, x: f64) -> Self;
    /// The number held, for branching. Calculations that branch on values are deterministic;
    /// reading a path vector here is a defect in the caller.
    fn primal(self) -> f64;
    fn exp(self) -> Self;
    fn ln(self) -> Self;
    fn sqrt(self) -> Self;
    fn abs(self) -> Self;
    fn recip(self) -> Self;
    /// `self` raised to a constant.
    fn powf(self, exponent: f64) -> Self;
    fn max(self, other: Self) -> Self;
    fn min(self, other: Self) -> Self;
    fn normal_pdf(self) -> Self;
    fn normal_cdf(self) -> Self;
}

impl Operand for f64 {
    fn like(self, x: f64) -> f64 {
        x
    }
    fn primal(self) -> f64 {
        self
    }
    fn exp(self) -> f64 {
        f64::exp(self)
    }
    fn ln(self) -> f64 {
        f64::ln(self)
    }
    fn sqrt(self) -> f64 {
        f64::sqrt(self)
    }
    fn abs(self) -> f64 {
        f64::abs(self)
    }
    fn recip(self) -> f64 {
        1.0 / self
    }
    fn powf(self, exponent: f64) -> f64 {
        f64::powf(self, exponent)
    }
    fn max(self, other: f64) -> f64 {
        f64::max(self, other)
    }
    fn min(self, other: f64) -> f64 {
        f64::min(self, other)
    }
    fn normal_pdf(self) -> f64 {
        normal_pdf(self)
    }
    fn normal_cdf(self) -> f64 {
        normal_cdf(self)
    }
}

/// How a node was computed. `x` and `y` are its arguments.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    /// An input or a constant: no arguments.
    Leaf,
    Add,
    Sub,
    Mul,
    Div,
    Neg,
    Exp,
    Ln,
    Sqrt,
    /// `x` raised to a constant.
    Powf(f64),
    Max,
    Min,
    Abs,
    NormalPdf,
    NormalCdf,
    /// 1 where `x` > 0 and 0 elsewhere. Its derivative is taken as 0.
    Step,
    /// The average of `x` over the paths.
    Average,
}

#[derive(Debug, Clone)]
struct Node {
    op: Op,
    x: u32,
    y: u32,
    value: Value,
}

/// The local work area of one recorded calculation.
#[derive(Debug, Default)]
pub struct Recorder {
    nodes: RefCell<Vec<Node>>,
}

/// A recorded value: its position in the recorder's arena.
#[derive(Debug, Clone, Copy)]
pub struct Var<'t> {
    recorder: &'t Recorder,
    id: u32,
}

impl PartialEq for Var<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.recorder, other.recorder) && self.id == other.id
    }
}

impl Recorder {
    pub fn new() -> Recorder {
        Recorder::default()
    }

    /// The number of nodes recorded so far.
    pub fn len(&self) -> usize {
        self.nodes.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn push(&self, op: Op, x: u32, y: u32, value: Value) -> Var<'_> {
        let mut nodes = self.nodes.borrow_mut();
        let id = u32::try_from(nodes.len()).expect("fewer than 2^32 nodes");
        nodes.push(Node { op, x, y, value });
        Var { recorder: self, id }
    }

    /// An independent variable with one value.
    pub fn input(&self, x: f64) -> Var<'_> {
        self.push(Op::Leaf, 0, 0, Value::Scalar(x))
    }

    /// An independent variable with one value per path.
    pub fn input_paths(&self, paths: impl Into<Arc<[f64]>>) -> Var<'_> {
        self.push(Op::Leaf, 0, 0, Value::Paths(paths.into()))
    }

    /// A constant.
    pub fn constant(&self, x: f64) -> Var<'_> {
        self.input(x)
    }

    /// The value of a variable of this recorder.
    pub fn value(&self, v: Var<'_>) -> Value {
        self.nodes.borrow()[v.id as usize].value.clone()
    }

    fn check_output(&self, output: Var<'_>) -> Result<(), Error> {
        if self.value(output).is_paths() {
            return Err(Error::InvalidInput {
                reason: "the output of a gradient must be one number; average the paths first"
                    .into(),
            });
        }
        Ok(())
    }

    /// The derivatives of `output` with respect to `wrt`, with no further recording. For a
    /// variable with paths, the derivative is per path under the pairing of the module's header.
    pub fn gradient(&self, output: Var<'_>, wrt: &[Var<'_>]) -> Result<Vec<Value>, Error> {
        self.check_output(output)?;
        let nodes = self.nodes.borrow();
        let last = output.id as usize;
        let mut adjoints: Vec<Option<Value>> = vec![None; last + 1];
        adjoints[last] = Some(Value::Scalar(1.0));
        let mut keep = vec![false; nodes.len()];
        for v in wrt {
            keep[v.id as usize] = true;
        }
        for i in (0..=last).rev() {
            let Some(a) = adjoints[i].clone() else {
                continue;
            };
            pull_back_values(&nodes, i, &a, &mut adjoints);
            if !keep[i] {
                adjoints[i] = None;
            }
        }
        Ok(wrt
            .iter()
            .map(|v| {
                let id = v.id as usize;
                let zero = || Value::Scalar(0.0);
                adjoints.get(id).cloned().flatten().unwrap_or_else(zero)
            })
            .collect())
    }

    /// The derivatives of `output` as variables of the same recorder, so that they can be
    /// differentiated again. The backward pass is recorded after the existing nodes.
    pub fn gradient_vars<'t>(
        &'t self,
        output: Var<'t>,
        wrt: &[Var<'t>],
    ) -> Result<Vec<Var<'t>>, Error> {
        self.check_output(output)?;
        let last = output.id as usize;
        let mut adjoints: Vec<Option<Var<'t>>> = vec![None; last + 1];
        adjoints[last] = Some(self.constant(1.0));
        for i in (0..=last).rev() {
            let Some(a) = adjoints[i] else {
                continue;
            };
            let (op, x, y) = {
                let nodes = self.nodes.borrow();
                (nodes[i].op, nodes[i].x, nodes[i].y)
            };
            let (px, py) = (self.var(x), self.var(y));
            let v = self.var(i as u32);
            match op {
                Op::Leaf | Op::Step => {}
                Op::Add => {
                    self.add_adjoint(&mut adjoints, px, a);
                    self.add_adjoint(&mut adjoints, py, a);
                }
                Op::Sub => {
                    self.add_adjoint(&mut adjoints, px, a);
                    self.add_adjoint(&mut adjoints, py, -a);
                }
                Op::Mul => {
                    self.add_adjoint(&mut adjoints, px, a * py);
                    self.add_adjoint(&mut adjoints, py, a * px);
                }
                Op::Div => {
                    self.add_adjoint(&mut adjoints, px, a / py);
                    self.add_adjoint(&mut adjoints, py, -(a * px / (py * py)));
                }
                Op::Neg => self.add_adjoint(&mut adjoints, px, -a),
                Op::Exp => self.add_adjoint(&mut adjoints, px, a * v),
                Op::Ln => self.add_adjoint(&mut adjoints, px, a / px),
                Op::Sqrt => self.add_adjoint(&mut adjoints, px, a / (v * 2.0)),
                Op::Powf(e) => {
                    self.add_adjoint(&mut adjoints, px, a * Operand::powf(px, e - 1.0) * e)
                }
                Op::Max => {
                    let s = (px - py).step();
                    self.add_adjoint(&mut adjoints, px, a * s);
                    self.add_adjoint(&mut adjoints, py, a * (-s + 1.0));
                }
                Op::Min => {
                    let s = (py - px).step();
                    self.add_adjoint(&mut adjoints, px, a * s);
                    self.add_adjoint(&mut adjoints, py, a * (-s + 1.0));
                }
                Op::Abs => {
                    let s = px.step() * 2.0 - 1.0;
                    self.add_adjoint(&mut adjoints, px, a * s);
                }
                Op::NormalPdf => self.add_adjoint(&mut adjoints, px, -(a * px * v)),
                Op::NormalCdf => self.add_adjoint(&mut adjoints, px, a * Operand::normal_pdf(px)),
                Op::Average => self.add_adjoint(&mut adjoints, px, a),
            }
        }
        Ok(wrt
            .iter()
            .map(|v| {
                adjoints
                    .get(v.id as usize)
                    .copied()
                    .flatten()
                    .unwrap_or_else(|| self.constant(0.0))
            })
            .collect())
    }

    fn var(&self, id: u32) -> Var<'_> {
        Var { recorder: self, id }
    }

    /// Adds to an adjoint; a number used on many paths receives the average.
    fn add_adjoint<'t>(
        &'t self,
        adjoints: &mut [Option<Var<'t>>],
        target: Var<'t>,
        amount: Var<'t>,
    ) {
        let amount = if !self.value(target).is_paths() && self.value(amount).is_paths() {
            amount.average()
        } else {
            amount
        };
        let slot = &mut adjoints[target.id as usize];
        *slot = Some(match *slot {
            Some(current) => current + amount,
            None => amount,
        });
    }
}

fn add_value(adjoints: &mut [Option<Value>], nodes: &[Node], target: u32, amount: Value) {
    let target = target as usize;
    let amount = if !nodes[target].value.is_paths() && amount.is_paths() {
        Value::Scalar(amount.mean())
    } else {
        amount
    };
    adjoints[target] = Some(match &adjoints[target] {
        Some(current) => current.zip(&amount, |a, b| a + b),
        None => amount,
    });
}

/// One step of the backward pass, on numbers. It must agree with `Recorder::gradient_vars`; the
/// tests compare the two.
fn pull_back_values(nodes: &[Node], i: usize, a: &Value, adjoints: &mut [Option<Value>]) {
    let node = &nodes[i];
    let (x, y) = (node.x, node.y);
    let vx = &nodes[x as usize].value;
    let vy = &nodes[y as usize].value;
    match node.op {
        Op::Leaf | Op::Step => {}
        Op::Add => {
            add_value(adjoints, nodes, x, a.clone());
            add_value(adjoints, nodes, y, a.clone());
        }
        Op::Sub => {
            add_value(adjoints, nodes, x, a.clone());
            add_value(adjoints, nodes, y, a.map(|a| -a));
        }
        Op::Mul => {
            add_value(adjoints, nodes, x, a.zip(vy, |a, y| a * y));
            add_value(adjoints, nodes, y, a.zip(vx, |a, x| a * x));
        }
        Op::Div => {
            add_value(adjoints, nodes, x, a.zip(vy, |a, y| a / y));
            let d = vx.zip(vy, |x, y| x / (y * y));
            add_value(adjoints, nodes, y, a.zip(&d, |a, d| -(a * d)));
        }
        Op::Neg => add_value(adjoints, nodes, x, a.map(|a| -a)),
        Op::Exp => add_value(adjoints, nodes, x, a.zip(&node.value, |a, v| a * v)),
        Op::Ln => add_value(adjoints, nodes, x, a.zip(vx, |a, x| a / x)),
        Op::Sqrt => add_value(adjoints, nodes, x, a.zip(&node.value, |a, v| a / (v * 2.0))),
        Op::Powf(e) => {
            let d = vx.map(|x| x.powf(e - 1.0) * e);
            add_value(adjoints, nodes, x, a.zip(&d, |a, d| a * d));
        }
        Op::Max => {
            let s = vx.zip(vy, |x, y| if x - y > 0.0 { 1.0 } else { 0.0 });
            add_value(adjoints, nodes, x, a.zip(&s, |a, s| a * s));
            add_value(adjoints, nodes, y, a.zip(&s, |a, s| a * (1.0 - s)));
        }
        Op::Min => {
            let s = vx.zip(vy, |x, y| if y - x > 0.0 { 1.0 } else { 0.0 });
            add_value(adjoints, nodes, x, a.zip(&s, |a, s| a * s));
            add_value(adjoints, nodes, y, a.zip(&s, |a, s| a * (1.0 - s)));
        }
        Op::Abs => {
            let s = vx.map(|x| if x > 0.0 { 1.0 } else { -1.0 });
            add_value(adjoints, nodes, x, a.zip(&s, |a, s| a * s));
        }
        Op::NormalPdf => {
            let d = vx.zip(&node.value, |x, v| -x * v);
            add_value(adjoints, nodes, x, a.zip(&d, |a, d| a * d));
        }
        Op::NormalCdf => {
            let d = vx.map(normal_pdf);
            add_value(adjoints, nodes, x, a.zip(&d, |a, d| a * d));
        }
        Op::Average => add_value(adjoints, nodes, x, a.clone()),
    }
}

impl<'t> Var<'t> {
    fn unary(self, op: Op, f: impl Fn(f64) -> f64 + Send + Sync) -> Var<'t> {
        let value = self.recorder.nodes.borrow()[self.id as usize].value.map(f);
        self.recorder.push(op, self.id, 0, value)
    }

    fn binary(self, other: Var<'t>, op: Op, f: impl Fn(f64, f64) -> f64 + Send + Sync) -> Var<'t> {
        assert!(
            std::ptr::eq(self.recorder, other.recorder),
            "variables of different recorders"
        );
        let value = {
            let nodes = self.recorder.nodes.borrow();
            nodes[self.id as usize]
                .value
                .zip(&nodes[other.id as usize].value, f)
        };
        self.recorder.push(op, self.id, other.id, value)
    }

    /// The value of this variable.
    pub fn value(self) -> Value {
        self.recorder.value(self)
    }

    /// 1 where the value is above 0 and 0 elsewhere, with a derivative of 0.
    pub fn step(self) -> Var<'t> {
        self.unary(Op::Step, |x| if x > 0.0 { 1.0 } else { 0.0 })
    }

    /// The average over the paths, as one number.
    pub fn average(self) -> Var<'t> {
        let value = Value::Scalar(self.recorder.nodes.borrow()[self.id as usize].value.mean());
        self.recorder.push(Op::Average, self.id, 0, value)
    }
}

impl<'t> Operand for Var<'t> {
    fn like(self, x: f64) -> Var<'t> {
        self.recorder.constant(x)
    }
    fn primal(self) -> f64 {
        self.value()
            .scalar()
            .expect("a deterministic calculation reads one number, not paths")
    }
    fn exp(self) -> Var<'t> {
        self.unary(Op::Exp, f64::exp)
    }
    fn ln(self) -> Var<'t> {
        self.unary(Op::Ln, f64::ln)
    }
    fn sqrt(self) -> Var<'t> {
        self.unary(Op::Sqrt, f64::sqrt)
    }
    fn abs(self) -> Var<'t> {
        self.unary(Op::Abs, f64::abs)
    }
    fn recip(self) -> Var<'t> {
        self.like(1.0) / self
    }
    fn powf(self, exponent: f64) -> Var<'t> {
        self.unary(Op::Powf(exponent), move |x| x.powf(exponent))
    }
    fn max(self, other: Var<'t>) -> Var<'t> {
        self.binary(other, Op::Max, f64::max)
    }
    fn min(self, other: Var<'t>) -> Var<'t> {
        self.binary(other, Op::Min, f64::min)
    }
    fn normal_pdf(self) -> Var<'t> {
        self.unary(Op::NormalPdf, normal_pdf)
    }
    fn normal_cdf(self) -> Var<'t> {
        self.unary(Op::NormalCdf, normal_cdf)
    }
}

impl<'t> Add for Var<'t> {
    type Output = Var<'t>;
    fn add(self, other: Var<'t>) -> Var<'t> {
        self.binary(other, Op::Add, |a, b| a + b)
    }
}
impl<'t> Sub for Var<'t> {
    type Output = Var<'t>;
    fn sub(self, other: Var<'t>) -> Var<'t> {
        self.binary(other, Op::Sub, |a, b| a - b)
    }
}
impl<'t> Mul for Var<'t> {
    type Output = Var<'t>;
    fn mul(self, other: Var<'t>) -> Var<'t> {
        self.binary(other, Op::Mul, |a, b| a * b)
    }
}
impl<'t> Div for Var<'t> {
    type Output = Var<'t>;
    fn div(self, other: Var<'t>) -> Var<'t> {
        self.binary(other, Op::Div, |a, b| a / b)
    }
}
impl<'t> Neg for Var<'t> {
    type Output = Var<'t>;
    fn neg(self) -> Var<'t> {
        self.unary(Op::Neg, |a| -a)
    }
}
impl<'t> Add<f64> for Var<'t> {
    type Output = Var<'t>;
    fn add(self, other: f64) -> Var<'t> {
        self + self.like(other)
    }
}
impl<'t> Sub<f64> for Var<'t> {
    type Output = Var<'t>;
    fn sub(self, other: f64) -> Var<'t> {
        self - self.like(other)
    }
}
impl<'t> Mul<f64> for Var<'t> {
    type Output = Var<'t>;
    fn mul(self, other: f64) -> Var<'t> {
        self * self.like(other)
    }
}
impl<'t> Div<f64> for Var<'t> {
    type Output = Var<'t>;
    fn div(self, other: f64) -> Var<'t> {
        self / self.like(other)
    }
}

/// The value and first derivatives of a scalar function at `point`.
pub fn value_and_gradient<F>(point: &[f64], f: F) -> Result<(f64, Vec<f64>), Error>
where
    F: for<'t> Fn(&[Var<'t>]) -> Result<Var<'t>, Error>,
{
    let recorder = Recorder::new();
    let inputs: Vec<Var> = point.iter().map(|&x| recorder.input(x)).collect();
    let output = f(&inputs)?;
    let gradient = recorder.gradient(output, &inputs)?;
    Ok((output.primal(), gradient.iter().map(Value::mean).collect()))
}

/// A value, its first derivatives and the rows of its table of second derivatives.
pub type ValueGradientHessian = (f64, Vec<f64>, Vec<Vec<f64>>);

/// The value, first and second derivatives of a scalar function at `point`.
pub fn value_gradient_hessian<F>(point: &[f64], f: F) -> Result<ValueGradientHessian, Error>
where
    F: for<'t> Fn(&[Var<'t>]) -> Result<Var<'t>, Error>,
{
    let recorder = Recorder::new();
    let inputs: Vec<Var> = point.iter().map(|&x| recorder.input(x)).collect();
    let output = f(&inputs)?;
    let first = recorder.gradient_vars(output, &inputs)?;
    let mut hessian = Vec::with_capacity(first.len());
    for derivative in &first {
        let row = recorder.gradient(derivative.average(), &inputs)?;
        hessian.push(row.iter().map(Value::mean).collect());
    }
    Ok((
        output.primal(),
        first.iter().map(|d| d.value().mean()).collect(),
        hessian,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numeric_gradient(f: impl Fn(&[f64]) -> f64, x: &[f64]) -> Vec<f64> {
        (0..x.len())
            .map(|i| {
                let h = 1e-6 * x[i].abs().max(1.0);
                let (mut up, mut down) = (x.to_vec(), x.to_vec());
                up[i] += h;
                down[i] -= h;
                (f(&up) - f(&down)) / (2.0 * h)
            })
            .collect()
    }

    fn generic<T: Operand>(v: &[T]) -> T {
        let (a, b, c) = (v[0], v[1], v[2]);
        let d = (a * b).exp() + (b / c).ln() * a - c.sqrt() / (a + 1.5) + a.normal_cdf();
        let e = a.powf(2.5) * Operand::max(b, c) - Operand::min(a, c) * (-b).abs();
        (d * e).normal_pdf() + e.recip() + (a - b) * 0.25 + (c + 2.0) / 3.0 - 1.0
    }

    #[test]
    fn every_operation_against_numeric_differences() {
        for point in [[0.7, 1.3, 0.4], [1.1, 0.2, 0.9], [2.0, 1.7, 3.0]] {
            let (value, gradient) = value_and_gradient(&point, |v| Ok(generic(v))).unwrap();
            assert_eq!(value, generic(&point));
            let numeric = numeric_gradient(generic::<f64>, &point);
            for (g, n) in gradient.iter().zip(&numeric) {
                assert!((g - n).abs() < 1e-7 * (1.0 + n.abs()), "{g} {n}");
            }
        }
    }

    #[test]
    fn numbers_and_recorded_backward_pass_agree() {
        let point = [0.7, 1.3, 0.4];
        let (_, by_numbers) = value_and_gradient(&point, |v| Ok(generic(v))).unwrap();
        let (_, by_graph, _) = value_gradient_hessian(&point, |v| Ok(generic(v))).unwrap();
        assert_eq!(by_numbers, by_graph);
    }

    #[test]
    fn second_derivatives_against_numeric_differences() {
        for point in [[0.7, 1.3, 0.4], [1.1, 0.2, 0.9]] {
            let (_, _, hessian) = value_gradient_hessian(&point, |v| Ok(generic(v))).unwrap();
            for i in 0..3 {
                let row = numeric_gradient(
                    |x| {
                        let (_, g) = value_and_gradient(x, |v| Ok(generic(v))).unwrap();
                        g[i]
                    },
                    &point,
                );
                for j in 0..3 {
                    assert!(
                        (hessian[i][j] - row[j]).abs() < 1e-6 * (1.0 + row[j].abs()),
                        "{i} {j}: {} {}",
                        hessian[i][j],
                        row[j]
                    );
                    assert!((hessian[i][j] - hessian[j][i]).abs() < 1e-9 * (1.0 + row[j].abs()));
                }
            }
        }
    }

    #[test]
    fn paths_use_the_expectation_pairing() {
        // E[(s z)^2] = s^2 E[z^2]: the derivative in s is the average over the paths
        let z = [0.5, -1.0, 2.0, 0.25];
        let recorder = Recorder::new();
        let s = recorder.input(1.5);
        let paths = recorder.input_paths(z.to_vec());
        let price = (s * paths * s * paths).average();
        let g = recorder.gradient(price, &[s, paths]).unwrap();
        let second_moment = z.iter().map(|x| x * x).sum::<f64>() / 4.0;
        assert_eq!(g[0], Value::Scalar(2.0 * 1.5 * second_moment));
        let per_path: Vec<f64> = z.iter().map(|x| 2.0 * 1.5 * 1.5 * x).collect();
        assert_eq!(g[1], Value::Paths(per_path.into()));
    }

    #[test]
    fn a_path_output_must_be_averaged_first() {
        let recorder = Recorder::new();
        let paths = recorder.input_paths(vec![1.0, 2.0]);
        assert!(recorder.gradient(paths, &[paths]).is_err());
        assert!(recorder.gradient_vars(paths, &[paths]).is_err());
    }

    #[test]
    fn inputs_that_do_not_reach_the_output_have_zero_derivatives() {
        let recorder = Recorder::new();
        let (a, b) = (recorder.input(2.0), recorder.input(3.0));
        let late = recorder.input(5.0);
        let g = recorder.gradient(a * a, &[a, b, late]).unwrap();
        assert_eq!(g, vec![Value::Scalar(4.0), Value::Scalar(0.0), Value::Scalar(0.0)]);
    }

    #[test]
    fn values_are_sendable_between_threads() {
        fn sendable<T: Send + Sync>() {}
        sendable::<Value>();
        fn moved<T: Send>() {}
        moved::<Recorder>();
    }
}
