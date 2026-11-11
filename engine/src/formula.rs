//! The little expression language of the formula sender unit: arithmetic, comparisons, logic and a list of
//! named functions over the three knobs `a`, `b`, `c`, the song position and the clock. Names are matched
//! without regard to case. The language is described in the public manual; the code here is our own.

/// What a formula can read while it runs.
#[derive(Debug, Clone, Copy, Default)]
pub struct Vars {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    /// Song position in quarter notes.
    pub song: f64,
    pub tempo: f64,
    /// Clock time in milliseconds.
    pub ms: f64,
    /// A fresh random number 0..1 for this run.
    pub rand: f64,
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Num(f64),
    Var(u8),
    Un(u8, Box<Node>),
    Bin(u8, Box<Node>, Box<Node>),
    Call(Fun, Vec<Node>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Fun {
    Abs, ArcCos, ArcCosh, ArcSin, ArcSinh, ArcTg, ArcTanh, Atan2, Case, Cos, Cosh, CoSec, DegToRad, Ceil, Ctg, Exp, Floor, Fold, Frac,
    GetTempo, If, IfE, IfG, IfGE, IfL, IfLE, Int, IntPower, Ln, Log10, Log2, LogN, Map, Max, Min, Neg, Pi, Power, RadToDeg, Rand,
    Round, Sec, SeededRand, Sin, Sinh, Sqrt, Sqr, Sum, Tg, Tanh, Trunc, Warp, SongTime, Time, Tension, Date, Mouse,
}

/// Name, function, fewest and most arguments.
const FUNS: &[(&str, Fun, usize, usize)] = &[
    ("abs", Fun::Abs, 1, 1), ("arccos", Fun::ArcCos, 1, 1), ("arccosh", Fun::ArcCosh, 1, 1), ("arcsin", Fun::ArcSin, 1, 1), ("arcsinh", Fun::ArcSinh, 1, 1),
    ("arctg", Fun::ArcTg, 1, 1), ("arctan", Fun::ArcTg, 1, 1), ("arctanh", Fun::ArcTanh, 1, 1), ("atan2", Fun::Atan2, 2, 2), ("case", Fun::Case, 3, 3),
    ("cos", Fun::Cos, 1, 1), ("cosh", Fun::Cosh, 1, 1), ("cosec", Fun::CoSec, 1, 1), ("degtorad", Fun::DegToRad, 1, 1), ("ceil", Fun::Ceil, 1, 1),
    ("ctg", Fun::Ctg, 1, 1), ("exp", Fun::Exp, 1, 1), ("floor", Fun::Floor, 1, 1), ("fold", Fun::Fold, 1, 1), ("frac", Fun::Frac, 1, 1),
    ("gettempo", Fun::GetTempo, 0, 0), ("if", Fun::If, 3, 3), ("ife", Fun::IfE, 2, 2), ("ifg", Fun::IfG, 2, 2), ("ifge", Fun::IfGE, 2, 2),
    ("ifl", Fun::IfL, 2, 2), ("ifle", Fun::IfLE, 2, 2), ("int", Fun::Int, 1, 1), ("intpower", Fun::IntPower, 2, 2), ("ln", Fun::Ln, 1, 1),
    ("log10", Fun::Log10, 1, 1), ("log2", Fun::Log2, 1, 1), ("logn", Fun::LogN, 2, 2), ("map", Fun::Map, 3, 3), ("max", Fun::Max, 1, 16),
    ("min", Fun::Min, 1, 16), ("neg", Fun::Neg, 1, 1), ("pi", Fun::Pi, 0, 0), ("power", Fun::Power, 2, 2), ("pow", Fun::Power, 2, 2),
    ("radtodeg", Fun::RadToDeg, 1, 1), ("rand", Fun::Rand, 0, 0), ("round", Fun::Round, 1, 1), ("sec", Fun::Sec, 1, 1), ("seededrand", Fun::SeededRand, 1, 1),
    ("sin", Fun::Sin, 1, 1), ("sinh", Fun::Sinh, 1, 1), ("sqrt", Fun::Sqrt, 1, 1), ("sqr", Fun::Sqr, 1, 1), ("sum", Fun::Sum, 1, 16),
    ("tg", Fun::Tg, 1, 1), ("tan", Fun::Tg, 1, 1), ("tanh", Fun::Tanh, 1, 1), ("trunc", Fun::Trunc, 1, 1), ("warp", Fun::Warp, 2, 2),
    ("songtime", Fun::SongTime, 0, 0), ("time", Fun::Time, 0, 0), ("tension", Fun::Tension, 2, 2), ("date", Fun::Date, 0, 0),
    ("mousex", Fun::Mouse, 0, 0), ("mousey", Fun::Mouse, 0, 0),
];

/// A formula ready to run.
#[derive(Debug, Clone, PartialEq)]
pub struct Formula(Node);

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Word(String),
    Op(&'static str),
}

fn tokens(src: &str) -> Result<Vec<Tok>, String> {
    let c: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch.is_ascii_digit() || (ch == '.' && c.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let from = i;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                i += 1;
            }
            // an exponent, but not the start of a name such as "2 else"
            if i < c.len() && (c[i] == 'e' || c[i] == 'E') {
                let mut j = i + 1;
                if j < c.len() && (c[j] == '+' || c[j] == '-') {
                    j += 1;
                }
                if j < c.len() && c[j].is_ascii_digit() {
                    while j < c.len() && c[j].is_ascii_digit() {
                        j += 1;
                    }
                    i = j;
                }
            }
            let s: String = c[from..i].iter().collect();
            out.push(Tok::Num(s.parse().map_err(|_| format!("\"{s}\" is not a number"))?));
        } else if ch.is_alphabetic() || ch == '_' {
            let from = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            out.push(Tok::Word(c[from..i].iter().collect::<String>().to_lowercase()));
        } else {
            let two: String = c[i..(i + 2).min(c.len())].iter().collect();
            let op = ["<>", ">=", "<="].iter().find(|o| **o == two).copied().or_else(|| ["+", "-", "*", "/", "^", "=", ">", "<", "%", "!", "(", ")", ","].iter().find(|o| o.starts_with(ch)).copied());
            match op {
                Some(o) => {
                    i += o.len();
                    out.push(Tok::Op(o));
                }
                None => return Err(format!("\"{ch}\" is not understood")),
            }
        }
    }
    Ok(out)
}

struct Parser {
    t: Vec<Tok>,
    at: usize,
}

// operator codes held in the tree
const ADD: u8 = 0;
const SUB: u8 = 1;
const MUL: u8 = 2;
const DIV: u8 = 3;
const POW: u8 = 4;
const EQ: u8 = 5;
const NE: u8 = 6;
const GT: u8 = 7;
const GE: u8 = 8;
const LT: u8 = 9;
const LE: u8 = 10;
const IDIV: u8 = 11;
const MOD: u8 = 12;
const FMOD: u8 = 13;
const OR: u8 = 14;
const AND: u8 = 15;
const XOR: u8 = 16;
const SHR: u8 = 17;
const SHL: u8 = 18;
const NEG: u8 = 0;
const NOT: u8 = 1;
const PCT: u8 = 2;
const FACT: u8 = 3;

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.at)
    }
    fn op(&mut self, o: &str) -> bool {
        let hit = matches!(self.peek(), Some(Tok::Op(x)) if *x == o);
        if hit {
            self.at += 1;
        }
        hit
    }
    fn word(&mut self, w: &str) -> bool {
        let hit = matches!(self.peek(), Some(Tok::Word(x)) if x == w);
        if hit {
            self.at += 1;
        }
        hit
    }
    fn bin(op: u8, l: Node, r: Node) -> Node {
        Node::Bin(op, Box::new(l), Box::new(r))
    }
    // loosest first: or, xor; and; comparisons; sums; products; power; signs; trailing marks
    fn logic(&mut self) -> Result<Node, String> {
        let mut l = self.conj()?;
        loop {
            let op = if self.word("or") { OR } else if self.word("xor") { XOR } else { return Ok(l) };
            l = Self::bin(op, l, self.conj()?);
        }
    }
    fn conj(&mut self) -> Result<Node, String> {
        let mut l = self.compare()?;
        while self.word("and") {
            l = Self::bin(AND, l, self.compare()?);
        }
        Ok(l)
    }
    fn compare(&mut self) -> Result<Node, String> {
        let mut l = self.sum()?;
        loop {
            let op = if self.op("<>") { NE } else if self.op(">=") { GE } else if self.op("<=") { LE } else if self.op("=") || self.word("in") { EQ } else if self.op(">") { GT } else if self.op("<") { LT } else { return Ok(l) };
            l = Self::bin(op, l, self.sum()?);
        }
    }
    fn sum(&mut self) -> Result<Node, String> {
        let mut l = self.product()?;
        loop {
            let op = if self.op("+") { ADD } else if self.op("-") { SUB } else { return Ok(l) };
            l = Self::bin(op, l, self.product()?);
        }
    }
    fn product(&mut self) -> Result<Node, String> {
        let mut l = self.sign()?;
        loop {
            let op = if self.op("*") { MUL } else if self.op("/") { DIV } else if self.word("div") { IDIV } else if self.word("mod") { MOD } else if self.word("fmod") { FMOD } else if self.word("shr") { SHR } else if self.word("shl") { SHL } else { return Ok(l) };
            l = Self::bin(op, l, self.sign()?);
        }
    }
    fn sign(&mut self) -> Result<Node, String> {
        if self.op("-") {
            return Ok(Node::Un(NEG, Box::new(self.sign()?)));
        }
        if self.op("+") {
            return self.sign();
        }
        if self.word("not") {
            return Ok(Node::Un(NOT, Box::new(self.sign()?)));
        }
        self.power()
    }
    fn power(&mut self) -> Result<Node, String> {
        let l = self.marked()?;
        if self.op("^") {
            // 2^3^2 is 2^(3^2), and 2^-1 is allowed
            return Ok(Self::bin(POW, l, self.sign()?));
        }
        Ok(l)
    }
    fn marked(&mut self) -> Result<Node, String> {
        let mut l = self.atom()?;
        loop {
            let op = if self.op("%") { PCT } else if self.op("!") { FACT } else { return Ok(l) };
            l = Node::Un(op, Box::new(l));
        }
    }
    fn atom(&mut self) -> Result<Node, String> {
        match self.t.get(self.at).cloned() {
            Some(Tok::Num(v)) => {
                self.at += 1;
                Ok(Node::Num(v))
            }
            Some(Tok::Op("(")) => {
                self.at += 1;
                let n = self.logic()?;
                if !self.op(")") {
                    return Err("a closing bracket is missing".into());
                }
                Ok(n)
            }
            Some(Tok::Word(w)) => {
                self.at += 1;
                if let Some(k) = ["a", "b", "c"].iter().position(|v| *v == w) {
                    return Ok(Node::Var(k as u8));
                }
                let Some(&(_, f, least, most)) = FUNS.iter().find(|f| f.0 == w) else { return Err(format!("\"{w}\" is not a known name")) };
                let mut args = Vec::new();
                if self.op("(") && !self.op(")") {
                    loop {
                        args.push(self.logic()?);
                        if self.op(")") {
                            break;
                        }
                        if !self.op(",") {
                            return Err(format!("a comma or closing bracket is missing after {w}"));
                        }
                    }
                }
                if args.len() < least || args.len() > most {
                    return Err(if least == most { format!("{w} takes {least} value{}", if least == 1 { "" } else { "s" }) } else { format!("{w} takes {least} to {most} values") });
                }
                Ok(Node::Call(f, args))
            }
            Some(Tok::Op(o)) => Err(format!("\"{o}\" is out of place")),
            None => Err("the formula ends too soon".into()),
        }
    }
}

pub fn compile(src: &str) -> Result<Formula, String> {
    let t = tokens(src)?;
    if t.is_empty() {
        return Err("the formula is empty".into());
    }
    let mut p = Parser { t, at: 0 };
    let n = p.logic()?;
    match p.peek() {
        None => Ok(Formula(n)),
        Some(Tok::Op(o)) => Err(format!("\"{o}\" is out of place")),
        Some(Tok::Word(w)) => Err(format!("\"{w}\" is out of place")),
        Some(Tok::Num(v)) => Err(format!("\"{v}\" is out of place")),
    }
}

fn truth(v: bool) -> f64 {
    if v { 1.0 } else { 0.0 }
}

/// A steady number 0..1 for a seed.
fn seeded(seed: f64) -> f64 {
    let mut x = seed.to_bits() ^ 0x9e37_79b9_7f4a_7c15;
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    (x >> 11) as f64 / (1u64 << 53) as f64
}

fn run(n: &Node, v: &Vars) -> f64 {
    match n {
        Node::Num(x) => *x,
        Node::Var(0) => v.a,
        Node::Var(1) => v.b,
        Node::Var(_) => v.c,
        Node::Un(op, x) => {
            let x = run(x, v);
            match *op {
                NEG => -x,
                NOT => truth(x <= 0.5),
                PCT => x / 100.0,
                _ => {
                    let k = x.floor().clamp(0.0, 170.0) as u32;
                    (1..=k).fold(1.0, |f, i| f * i as f64)
                }
            }
        }
        Node::Bin(op, l, r) => {
            let (l, r) = (run(l, v), run(r, v));
            match *op {
                ADD => l + r,
                SUB => l - r,
                MUL => l * r,
                DIV => l / r,
                POW => l.powf(r),
                EQ => truth(l == r),
                NE => truth(l != r),
                GT => truth(l > r),
                GE => truth(l >= r),
                LT => truth(l < r),
                LE => truth(l <= r),
                IDIV => (l / r).trunc(),
                MOD => (l.trunc() % r.trunc()).trunc(),
                FMOD => l % r,
                OR => truth(l > 0.5 || r > 0.5),
                AND => truth(l > 0.5 && r > 0.5),
                XOR => truth((l > 0.5) != (r > 0.5)),
                SHR => (l.trunc() / 2f64.powf(r.trunc())).trunc(),
                _ => l.trunc() * 2f64.powf(r.trunc()),
            }
        }
        Node::Call(f, args) => {
            let x: Vec<f64> = args.iter().map(|a| run(a, v)).collect();
            let g = |i: usize| x.get(i).copied().unwrap_or(0.0);
            match f {
                Fun::Abs => g(0).abs(),
                Fun::ArcCos => g(0).acos(),
                Fun::ArcCosh => g(0).acosh(),
                Fun::ArcSin => g(0).asin(),
                Fun::ArcSinh => g(0).asinh(),
                Fun::ArcTg => g(0).atan(),
                Fun::ArcTanh => g(0).atanh(),
                Fun::Atan2 => g(0).atan2(g(1)),
                Fun::Case => if g(0) == 1.0 { g(1) } else { g(2) },
                Fun::Cos => g(0).cos(),
                Fun::Cosh => g(0).cosh(),
                Fun::CoSec => 1.0 / g(0).sin(),
                Fun::DegToRad => g(0).to_radians(),
                Fun::Ceil => g(0).ceil(),
                Fun::Ctg => 1.0 / g(0).tan(),
                Fun::Exp => g(0).exp(),
                Fun::Floor => g(0).floor(),
                Fun::Fold => {
                    // bounce between 0 and 1
                    let t = g(0).rem_euclid(2.0);
                    if t > 1.0 { 2.0 - t } else { t }
                }
                Fun::Frac => g(0).fract(),
                Fun::GetTempo => v.tempo,
                Fun::If => if g(0) > 0.5 { g(1) } else { g(2) },
                Fun::IfE => truth(g(0) == g(1)),
                Fun::IfG => truth(g(0) > g(1)),
                Fun::IfGE => truth(g(0) >= g(1)),
                Fun::IfL => truth(g(0) < g(1)),
                Fun::IfLE => truth(g(0) <= g(1)),
                Fun::Int => g(0).trunc(),
                Fun::IntPower => g(0).powi(g(1).trunc().clamp(-1024.0, 1024.0) as i32),
                Fun::Ln => g(0).ln(),
                Fun::Log10 => g(0).log10(),
                Fun::Log2 => g(0).log2(),
                Fun::LogN => g(1).ln() / g(0).ln(),
                Fun::Map => g(0) * (g(2) - g(1)) + g(1),
                Fun::Max => x.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                Fun::Min => x.iter().copied().fold(f64::INFINITY, f64::min),
                Fun::Neg => -g(0),
                Fun::Pi => std::f64::consts::PI,
                Fun::Power => g(0).powf(g(1)),
                Fun::RadToDeg => g(0).to_degrees(),
                Fun::Rand => v.rand,
                Fun::Round => g(0).round(),
                Fun::Sec => 1.0 / g(0).cos(),
                Fun::SeededRand => seeded(g(0)),
                Fun::Sin => g(0).sin(),
                Fun::Sinh => g(0).sinh(),
                Fun::Sqrt => g(0).sqrt(),
                Fun::Sqr => g(0) * g(0),
                Fun::Sum => x.iter().sum(),
                Fun::Tg => g(0).tan(),
                Fun::Tanh => g(0).tanh(),
                Fun::Trunc => g(0).trunc(),
                Fun::Warp => {
                    // the middle of the travel moves to the warp point
                    let (t, w) = (g(0).clamp(0.0, 1.0), g(1).clamp(1e-6, 1.0 - 1e-6));
                    if t < w { 0.5 * t / w } else { 0.5 + 0.5 * (t - w) / (1.0 - w) }
                }
                Fun::SongTime => v.song,
                Fun::Time => v.ms,
                Fun::Tension => {
                    let (t, k) = (g(0).clamp(0.0, 1.0), g(1));
                    if k.abs() < 1e-9 { t } else { ((k * t).exp() - 1.0) / (k.exp() - 1.0) }
                }
                Fun::Date => v.ms / 86_400_000.0,
                Fun::Mouse => 0.0,
            }
        }
    }
}

impl Formula {
    /// The formula's value as it stands, which may lie outside what a control takes.
    pub fn value(&self, v: &Vars) -> f64 {
        run(&self.0, v)
    }
    /// The value as control travel: cut to 0..=1, and 0 when it is not a number.
    pub fn out(&self, v: &Vars) -> f32 {
        let x = self.value(v);
        if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) as f32 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn val(src: &str, v: &Vars) -> f64 {
        compile(src).unwrap_or_else(|e| panic!("{src}: {e}")).value(v)
    }

    #[test]
    fn sums_follow_the_usual_order() {
        let v = Vars { a: 0.25, b: 0.5, c: 2.0, ..Default::default() };
        assert_eq!(val("a+b+c", &v), 2.75);
        assert_eq!(val("1+2*3^2", &v), 19.0);
        assert_eq!(val("-(1+2)*3", &v), -9.0);
        assert_eq!(val("2^-1", &v), 0.5);
        assert_eq!(val("50%", &v), 0.5);
        assert_eq!(val("4!", &v), 24.0);
        assert_eq!(val("7 div 2 + 7 mod 2", &v), 4.0);
        assert_eq!(val("1 shl 4", &v), 16.0);
        assert_eq!(val("1.5e2", &v), 150.0);
    }

    #[test]
    fn logic_and_choices_work_on_the_half_way_mark() {
        let v = Vars { a: 0.7, b: 0.2, c: 0.0, ..Default::default() };
        assert_eq!(val("a > b", &v), 1.0);
        assert_eq!(val("a and b", &v), 0.0);
        assert_eq!(val("a or b", &v), 1.0);
        assert_eq!(val("not b", &v), 1.0);
        assert_eq!(val("If(a, 3, 4)", &v), 3.0);
        assert_eq!(val("IF(B, 3, 4)", &v), 4.0);
        assert_eq!(val("a <> a", &v), 0.0);
        assert_eq!(val("Map(0.5, 2, 4)", &v), 3.0);
        assert_eq!(val("Max(a, b, 3)", &v), 3.0);
    }

    #[test]
    fn the_manual_sine_example_swings_between_zero_and_one() {
        let f = compile("0.5*Sin(SongTime*Pi*2+0.25)+0.5").unwrap();
        let mut seen = (1.0f32, 0.0f32);
        for k in 0..400 {
            let o = f.out(&Vars { song: k as f64 / 100.0, ..Default::default() });
            seen = (seen.0.min(o), seen.1.max(o));
        }
        assert!(seen.0 < 0.01 && seen.1 > 0.99, "{seen:?}");
        // names with and without brackets both read
        assert_eq!(compile("SongTime()*GetTempo").unwrap().value(&Vars { song: 2.0, tempo: 140.0, ..Default::default() }), 280.0);
    }

    #[test]
    fn mistakes_are_named() {
        assert!(compile("").is_err());
        assert!(compile("a+").unwrap_err().contains("ends too soon"));
        assert!(compile("wobble(a)").unwrap_err().contains("wobble"));
        assert!(compile("(a+b").unwrap_err().contains("bracket"));
        assert!(compile("Sin(a, b)").unwrap_err().contains("takes 1"));
        assert_eq!(compile("1/0-1/0").unwrap().out(&Vars::default()), 0.0);
    }
}
