use num_complex::Complex64;
use std::f64::consts::FRAC_1_SQRT_2;

pub type Amp = Complex64;

const fn c(re: f64, im: f64) -> Amp {
    Complex64 { re, im }
}

const ZERO: Amp = c(0.0, 0.0);
const ONE: Amp = c(1.0, 0.0);
const R: f64 = FRAC_1_SQRT_2;

const H_MAT: [[Amp; 2]; 2] = [[c(R, 0.0), c(R, 0.0)], [c(R, 0.0), c(-R, 0.0)]];
const X_MAT: [[Amp; 2]; 2] = [[ZERO, ONE], [ONE, ZERO]];
const Y_MAT: [[Amp; 2]; 2] = [[ZERO, c(0.0, -1.0)], [c(0.0, 1.0), ZERO]];
const Z_MAT: [[Amp; 2]; 2] = [[ONE, ZERO], [ZERO, c(-1.0, 0.0)]];
const S_MAT: [[Amp; 2]; 2] = [[ONE, ZERO], [ZERO, c(0.0, 1.0)]];
const T_MAT: [[Amp; 2]; 2] = [[ONE, ZERO], [ZERO, c(R, R)]];

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Op {
    H(usize),
    X(usize),
    Y(usize),
    Z(usize),
    S(usize),
    T(usize),
    Cnot { control: usize, target: usize },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Cell {
    H,
    X,
    Y,
    Z,
    S,
    T,
    CnotCtrl(usize),
    CnotTarg(usize),
}

impl Cell {
    pub fn symbol(self) -> char {
        match self {
            Cell::H => 'H',
            Cell::X => 'X',
            Cell::Y => 'Y',
            Cell::Z => 'Z',
            Cell::S => 'S',
            Cell::T => 'T',
            Cell::CnotCtrl(_) => '●',
            Cell::CnotTarg(_) => '⊕',
        }
    }
}

#[derive(Clone, Debug)]
pub struct State {
    pub qubits: usize,
    pub amps: Vec<Amp>,
}

impl State {
    pub fn zero(qubits: usize) -> Self {
        let mut amps = vec![ZERO; 1 << qubits];
        amps[0] = ONE;
        Self { qubits, amps }
    }

    pub fn probabilities(&self) -> Vec<f64> {
        self.amps.iter().map(|a| a.norm_sqr()).collect()
    }

    pub fn qubit_prob_one(&self, q: usize) -> f64 {
        let bit = 1 << q;
        self.amps
            .iter()
            .enumerate()
            .filter_map(|(i, a)| (i & bit != 0).then(|| a.norm_sqr()))
            .sum()
    }

    pub fn apply(&mut self, op: Op) {
        match op {
            Op::H(q) => apply_1q(self, q, &H_MAT),
            Op::X(q) => apply_1q(self, q, &X_MAT),
            Op::Y(q) => apply_1q(self, q, &Y_MAT),
            Op::Z(q) => apply_1q(self, q, &Z_MAT),
            Op::S(q) => apply_1q(self, q, &S_MAT),
            Op::T(q) => apply_1q(self, q, &T_MAT),
            Op::Cnot { control, target } => apply_cnot(self, control, target),
        }
    }
}

fn apply_1q(s: &mut State, q: usize, m: &[[Amp; 2]; 2]) {
    let stride = 1usize << q;
    let block = stride << 1;
    let mut base = 0;
    while base < s.amps.len() {
        for k in 0..stride {
            let a = base + k;
            let b = a + stride;
            let va = s.amps[a];
            let vb = s.amps[b];
            s.amps[a] = m[0][0] * va + m[0][1] * vb;
            s.amps[b] = m[1][0] * va + m[1][1] * vb;
        }
        base += block;
    }
}

fn apply_cnot(s: &mut State, ctrl: usize, targ: usize) {
    let cb = 1usize << ctrl;
    let tb = 1usize << targ;
    for i in 0..s.amps.len() {
        if i & cb != 0 && i & tb == 0 {
            s.amps.swap(i, i | tb);
        }
    }
}

#[derive(Clone, Debug)]
pub struct Circuit {
    pub qubits: usize,
    pub cols: Vec<Vec<Option<Cell>>>,
}

impl Circuit {
    pub fn new(qubits: usize, cols: usize) -> Self {
        Self {
            qubits,
            cols: vec![vec![None; qubits]; cols],
        }
    }

    pub fn place_single(&mut self, col: usize, q: usize, cell: Cell) {
        self.clear(col, q);
        self.cols[col][q] = Some(cell);
    }

    pub fn place_cnot(&mut self, col: usize, control: usize, target: usize) -> bool {
        if control == target || control >= self.qubits || target >= self.qubits {
            return false;
        }
        self.clear(col, control);
        self.clear(col, target);
        self.cols[col][control] = Some(Cell::CnotCtrl(target));
        self.cols[col][target] = Some(Cell::CnotTarg(control));
        true
    }

    pub fn clear(&mut self, col: usize, q: usize) {
        let partner = match self.cols[col][q] {
            Some(Cell::CnotCtrl(t)) => Some(t),
            Some(Cell::CnotTarg(c)) => Some(c),
            _ => None,
        };
        self.cols[col][q] = None;
        if let Some(p) = partner {
            self.cols[col][p] = None;
        }
    }

    pub fn ops(&self) -> Vec<Op> {
        let mut out = Vec::new();
        for col in &self.cols {
            for (q, cell) in col.iter().enumerate() {
                match *cell {
                    None | Some(Cell::CnotTarg(_)) => {}
                    Some(Cell::H) => out.push(Op::H(q)),
                    Some(Cell::X) => out.push(Op::X(q)),
                    Some(Cell::Y) => out.push(Op::Y(q)),
                    Some(Cell::Z) => out.push(Op::Z(q)),
                    Some(Cell::S) => out.push(Op::S(q)),
                    Some(Cell::T) => out.push(Op::T(q)),
                    Some(Cell::CnotCtrl(t)) => out.push(Op::Cnot { control: q, target: t }),
                }
            }
        }
        out
    }

    pub fn run(&self) -> State {
        let mut s = State::zero(self.qubits);
        for op in self.ops() {
            s.apply(op);
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-10
    }

    #[test]
    fn hadamard_superposition() {
        let mut s = State::zero(1);
        s.apply(Op::H(0));
        assert!(close(s.qubit_prob_one(0), 0.5));
    }

    #[test]
    fn bell_pair() {
        let mut c = Circuit::new(2, 2);
        c.place_single(0, 0, Cell::H);
        c.place_cnot(1, 0, 1);
        let s = c.run();
        let p = s.probabilities();
        assert!(close(p[0b00], 0.5));
        assert!(close(p[0b11], 0.5));
        assert!(close(p[0b01], 0.0));
        assert!(close(p[0b10], 0.0));
    }

    #[test]
    fn x_flips() {
        let mut s = State::zero(3);
        s.apply(Op::X(1));
        assert!(close(s.qubit_prob_one(1), 1.0));
        assert!(close(s.qubit_prob_one(0), 0.0));
    }

    #[test]
    fn hxh_equals_z_up_to_global_phase() {
        let mut a = State::zero(1);
        a.apply(Op::X(0));
        a.apply(Op::Z(0));
        let mut b = State::zero(1);
        b.apply(Op::X(0));
        b.apply(Op::H(0));
        b.apply(Op::X(0));
        b.apply(Op::H(0));
        for (x, y) in a.amps.iter().zip(&b.amps) {
            assert!(close(x.norm_sqr(), y.norm_sqr()));
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::{Op, State};
    use serde::{Deserialize, Serialize};
    use wasm_bindgen::prelude::*;

    #[derive(Deserialize)]
    #[serde(tag = "kind", rename_all = "lowercase")]
    enum OpInput {
        H { q: usize },
        X { q: usize },
        Y { q: usize },
        Z { q: usize },
        S { q: usize },
        T { q: usize },
        Cnot { control: usize, target: usize },
    }

    impl From<OpInput> for Op {
        fn from(o: OpInput) -> Self {
            match o {
                OpInput::H { q } => Op::H(q),
                OpInput::X { q } => Op::X(q),
                OpInput::Y { q } => Op::Y(q),
                OpInput::Z { q } => Op::Z(q),
                OpInput::S { q } => Op::S(q),
                OpInput::T { q } => Op::T(q),
                OpInput::Cnot { control, target } => Op::Cnot { control, target },
            }
        }
    }

    #[derive(Deserialize)]
    struct CircuitInput {
        qubits: usize,
        ops: Vec<OpInput>,
    }

    #[derive(Serialize)]
    struct RunResult {
        qubits: usize,
        probabilities: Vec<f64>,
        qubit_p_one: Vec<f64>,
    }

    #[wasm_bindgen]
    pub fn run_circuit(input: JsValue) -> Result<JsValue, JsValue> {
        let input: CircuitInput = serde_wasm_bindgen::from_value(input)?;
        let mut s = State::zero(input.qubits);
        for op in input.ops {
            s.apply(op.into());
        }
        let qubit_p_one = (0..s.qubits).map(|q| s.qubit_prob_one(q)).collect();
        let result = RunResult {
            qubits: s.qubits,
            probabilities: s.probabilities(),
            qubit_p_one,
        };
        serde_wasm_bindgen::to_value(&result).map_err(Into::into)
    }
}

