//! A small speaking voice of our own: written words become a wave. Speech preset files (plain text with a
//! speaker section: style, personality, pitch, rate, voicing mode and the words) are read and written here.
//!
//! The words are turned into sounds by spelling rules, and the sounds are sung by a pulse and noise source
//! through moving resonances, the classic formant way. The voice is ours; it does not try to copy any other
//! speech engine's tone, only to say the same words at the stored pitch and pace.

/// What a speech preset holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Voice {
    pub style: u8,
    pub personality: u8,
    /// Hz.
    pub pitch: f32,
    /// Words a minute.
    pub rate: f32,
    pub mode: u8,
    pub text: String,
}

impl Default for Voice {
    fn default() -> Self {
        Self { style: 0, personality: 0, pitch: 92.0, rate: 150.0, mode: 0, text: "Type your text here".into() }
    }
}

pub const HEAD: &str = "[Speaker Settings]";
pub const STYLES: [&str; 5] = ["Natural", "Natural, wide", "Flat", "Sung", "Wandering"];
pub const MODES: [&str; 3] = ["Voiced", "Breathy", "Whisper"];
pub const PERSONALITIES: [&str; 20] = [
    "Man", "Woman", "Big man", "Child", "Giant", "Soft woman", "Soft man", "Crisp man", "Insect", "Machine", "Visitor", "Titan", "Hurried", "Old woman", "Tiny", "Cave dweller", "Bookish", "Timid",
    "Unsteady", "Choir child",
];

/// Read a speech preset; None when the bytes are something else.
pub fn parse(bytes: &[u8]) -> Option<Voice> {
    if bytes.len() > 65_536 {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok().map(|s| s.to_string()).unwrap_or_else(|| bytes.iter().map(|&b| b as char).collect());
    if !text.trim_start_matches('\u{feff}').trim_start().starts_with(HEAD) {
        return None;
    }
    let mut v = Voice { text: String::new(), ..Voice::default() };
    for line in text.lines() {
        let Some((k, val)) = line.split_once('=') else { continue };
        let val = val.trim_end_matches(['\r', '\n']);
        match k.trim() {
            "Style" => v.style = val.trim().parse().unwrap_or(0),
            "Personality" => v.personality = val.trim().parse().unwrap_or(0),
            "Pitch" => v.pitch = val.trim().parse().unwrap_or(92.0),
            "Rate" => v.rate = val.trim().parse().unwrap_or(150.0),
            "VoicingMode" => v.mode = val.trim().parse().unwrap_or(0),
            "Text" => v.text = val.to_string(),
            _ => {}
        }
    }
    Some(v)
}

pub fn write(v: &Voice) -> String {
    format!("{HEAD}\r\nStyle={}\r\nPersonality={}\r\nPitch={}\r\nRate={}\r\nVoicingMode={}\r\nText={}\r\n", v.style, v.personality, v.pitch.round() as i32, v.rate.round() as i32, v.mode, v.text.replace(['\r', '\n'], " "))
}

// ---------------------------------------------------------------------------------------------------------
// words to sounds

/// Spelling rules: what stands before | the letters | what stands after = the sounds. In the surroundings
/// `#` is one or more vowels, `:` any run of consonants, `^` one consonant, `.` a voiced consonant, `+` one of
/// E I Y, `%` an ending (E, ER, ES, ED, ING, ELY) and a blank the edge of the word. The first rule that fits wins.
const RULES: &[&str] = &[
    // A
    " |A| =AX", " |ARE| =AA R", " |AR|O=AX R", "|AR|#=EH R", "^|AS|#=EY S", "|A|WA=AX", "|AW|=AO", " :|ANY|=EH N IY", "|A|^+#=EY", "#:|ALLY|=AX L IY", " |AL|#=AX L", "|AGAIN|=AX G EH N",
    "#:|AG|E=IH JH", "|A|^+:#=AE", " :|A|^+ =EY", "|A|^%=EY", " |ARR|=AX R", "|ARR|=AE R", " :|AR| =AA R", "|AR| =ER", "|AR|=AA R", "|AIR|=EH R", "|AI|=EY", "|AY|=EY", "|AU|=AO", "#:|AL| =AX L",
    "#:|ALS| =AX L Z", "|ALK|=AO K", "|AL|^=AO L", " :|ABLE|=EY B AX L", "|ABLE|=AX B AX L", "|ANG|+=EY N JH", "|A|=AE",
    // B
    " |BE|^#=B IH", "|BEING|=B IY IH NG", " |BOTH| =B OW TH", " |BUS|#=B IH Z", "|BUIL|=B IH L", "B|B|=", "|B|=B",
    // C
    " |CH|^=K", "^E|CH|=K", "|CH|=CH", " S|CI|#=S AY", "|CI|A=SH", "|CI|O=SH", "|CI|EN=SH", "|C|+=S", "|CK|=K", "|COM|%=K AH M", "C|C|+=S", "C|C|=", "|C|=K",
    // D
    "#:|DED| =D IH D", ".E|D| =D", "#^:E|D| =T", " |DE|^#=D IH", " |DO| =D UW", " |DOES|=D AH Z", " |DOING|=D UW IH NG", " |DOW|=D AW", "|DU|A=JH UW", "D|D|=", "|D|=D",
    // E
    "#:|E| =", "'^:|E| =", " :|E| =IY", "#|ED| =D", "#:|E|D =", "|EV|ER=EH V", "|E|^%=IY", "|ERI|#=IY R IY", "|ERI|=EH R IH", "#:|ER|#=ER", "|ER|#=EH R", "|ER|=ER", " |EVEN|=IY V EH N", "#:|E|W=",
    "T|EW|=UW", "S|EW|=UW", "R|EW|=UW", "D|EW|=UW", "L|EW|=UW", "Z|EW|=UW", "N|EW|=UW", "J|EW|=UW", "TH|EW|=UW", "CH|EW|=UW", "SH|EW|=UW", "|EW|=Y UW", "|E|O=IY", "#:S|ES| =IH Z", "#:C|ES| =IH Z",
    "#:G|ES| =IH Z", "#:Z|ES| =IH Z", "#:X|ES| =IH Z", "#:J|ES| =IH Z", "#:CH|ES| =IH Z", "#:SH|ES| =IH Z", "#:|E|S =", "#:|ELY| =L IY", "#:|EMENT|=M EH N T", "|EFUL|=F UH L", "|EE|=IY", "|EARN|=ER N",
    " |EAR|^=ER", "|EAD|=EH D", "#:|EA| =IY AX", "|EA|SU=EH", "|EA|=IY", "|EIGH|=EY", "|EI|=IY", " |EYE|=AY", "|EY|=IY", "|EU|=Y UW", "|E|=EH",
    // F
    "|FUL|=F UH L", "F|F|=", "|F|=F",
    // G
    "|GIV|=G IH V", " |G|I^=G", "|GE|T=G EH", "SU|GGES|=G JH EH S", "|GG|=G", " B#|G|=G", "|G|+=JH", "|GREAT|=G R EY T", "#|GH|=", "|G|=G",
    // H
    " |HAV|=HH AE V", " |HERE|=HH IY R", " |HOUR|=AW ER", "|HOW|=HH AW", "|H|#=HH", "|H|=",
    // I
    " |IN|=IH N", " |I| =AY", "|IN|D=AY N", "|IER|=IY ER", "#:R|IED|=IY D", "|IED| =AY D", "|IEN|=IY EH N", "|IE|T=AY EH", " :|I|%=AY", "|I|%=IY", "|IE|=IY", "|I|^+:#=IH", "|IR|#=AY R", "|IZ|%=AY Z",
    "|IS|%=AY Z", "|I|D%=AY", "+^|I|^+=IH", "|I|T%=AY", "#^:|I|^+=IH", "|I|^+=AY", "|IR|=ER", "|IGH|=AY", "|ILD|=AY L D", "|IGN| =AY N", "|IGN|^=AY N", "|IGN|%=AY N", "|IQUE|=IY K", "|I|=IH",
    // J K L M
    "|J|=JH", " |K|N=", "|K|=K", "|LO|C#=L OW", "L|L|=", "#^:|L|%=AX L", "|LEAD|=L IY D", "|L|=L", "|MOV|=M UW V", "M|M|=", "|M|=M",
    // N
    "E|NG|+=N JH", "|NG|R=NG G", "|NG|#=NG G", "|NGL|%=NG G AX L", "|NG|=NG", "|NK|=NG K", " |NOW| =N AW", "N|N|=", "|N|=N",
    // O
    "|OF| =AX V", "|OROUGH|=ER OW", "#:|OR| =ER", "#:|ORS| =ER Z", "|OR|=AO R", " |ONE|=W AH N", "|OW|=OW", " |OVER|=OW V ER", "|OV|=AH V", "|O|^%=OW", "|O|^EN=OW", "|O|^I#=OW", "|OL|D=OW L",
    "|OUGHT|=AO T", "|OUGH|=AH F", " |OU|=AW", "H|OU|S#=AW", "|OUS|=AX S", "|OUR|=AO R", "|OULD|=UH D", "^|OU|^L=AH", "|OUP|=UW P", "|OU|=AW", "|OY|=OY", "|OING|=OW IH NG", "|OI|=OY", "|OOR|=AO R",
    "|OOK|=UH K", "|OOD|=UH D", "|OO|=UW", "|O|E=OW", "|O| =OW", "|OA|=OW", " |ONLY|=OW N L IY", " |ONCE|=W AH N S", "|ON'T|=OW N T", "C|O|N=AA", "|O|NG=AO", " ^:|O|N=AH", "I|ON|=AX N", "#:|ON| =AX N",
    "#^|ON|=AX N", "|O|ST =OW", "|OF|^=AO F", "|OTHER|=AH DH ER", "|OSS| =AO S", "#^:|OM|=AH M", "|O|=AA",
    // P Q R
    "|PH|=F", "|PEOP|=P IY P", "|POW|=P AW", "|PUT| =P UH T", "P|P|=", "|P|=P", "|QUAR|=K W AO R", "|QU|=K W", "|Q|=K", " |RE|^#=R IY", "R|R|=", "|R|=R",
    // S
    "|SH|=SH", "#|SION|=ZH AX N", "|SOME|=S AH M", "#|SUR|#=ZH ER", "|SUR|#=SH ER", "#|SU|#=ZH UW", "#|SSU|#=SH UW", "#|SED| =Z D", "#|S|#=Z", "|SAID|=S EH D", "^|SION|=SH AX N", "|S|S=", ".|S| =Z",
    "#:.E|S| =Z", "#^:##|S| =Z", "#^:#|S| =S", "U|S| =S", " :#|S| =Z", " |SCH|=S K", "|S|C+=", "#|SM|=Z M", "#|SN|'=Z AX N", "|S|=S",
    // T
    " |THE| =DH AX", "|TO| =T UW", "|THAT| =DH AE T", " |THIS| =DH IH S", " |THEY|=DH EY", " |THERE|=DH EH R", "|THER|=DH ER", "|THEIR|=DH EH R", " |THAN| =DH AE N", " |THEM| =DH EH M",
    "|THESE| =DH IY Z", " |THEN|=DH EH N", "|THROUGH|=TH R UW", "|THOSE|=DH OW Z", "|THOUGH| =DH OW", " |THUS|=DH AH S", "|TH|=TH", "#:|TED| =T IH D", "S|TI|#N=CH", "|TI|O=SH", "|TI|A=SH",
    "|TIEN|=SH AX N", "|TUR|#=CH ER", "|TU|A=CH UW", " |TWO|=T UW", "T|T|=", "|T|=T",
    // U
    " |UN|I=Y UW N", " |UN|=AH N", " |UPON|=AX P AO N", "T|UR|#=UH R", "S|UR|#=UH R", "R|UR|#=UH R", "D|UR|#=UH R", "L|UR|#=UH R", "Z|UR|#=UH R", "N|UR|#=UH R", "J|UR|#=UH R", "TH|UR|#=UH R",
    "CH|UR|#=UH R", "SH|UR|#=UH R", "|UR|#=Y UH R", "|UR|=ER", "|U|^ =AH", "|U|^^=AH", "|UY|=AY", " G|U|#=", "G|U|%=", "G|U|#=W", "#N|U|=Y UW", "T|U|=UW", "S|U|=UW", "R|U|=UW", "D|U|=UW", "L|U|=UW",
    "Z|U|=UW", "N|U|=UW", "J|U|=UW", "TH|U|=UW", "CH|U|=UW", "SH|U|=UW", "|U|=Y UW",
    // V W X Y Z
    "|VIEW|=V Y UW", "|V|=V", " |WERE|=W ER", "|WA|S=W AA", "|WA|T=W AA", "|WHERE|=W EH R", "|WHAT|=W AA T", "|WHOL|=HH OW L", "|WHO|=HH UW", "|WH|=W", "|WAR|=W AO R", "|WOR|^=W ER", "|WR|=R",
    "|W|=W", "|X|=K S", "|YOUNG|=Y AH NG", " |YOUR|=Y AO R", " |YOU|=Y UW", " |YES|=Y EH S", " |Y|=Y", "#^:|Y| =IY", "#^:|Y|I=IY", " :|Y| =AY", " :|Y|#=AY", " :|Y|^+:#=IH", " :|Y|^#=AY", "|Y|=IH",
    "Z|Z|=", "|Z|=Z",
];

/// Words the rules get wrong often enough to matter.
const WORDS: &[(&str, &str)] = &[
    ("BASS", "B EY S"), ("BABY", "B EY B IY"), ("STUDIO", "S T UW D IY OW"), ("FRUITY", "F R UW T IY"), ("FRUIT", "F R UW T"), ("TECHNO", "T EH K N OW"), ("OK", "OW K EY"), ("DJ", "D IY JH EY"),
    ("MC", "EH M S IY"), ("HAVE", "HH AE V"), ("LOVE", "L AH V"), ("COME", "K AH M"), ("DONE", "D AH N"), ("GONE", "G AO N"), ("GIVE", "G IH V"), ("LIVE", "L IH V"), ("PEOPLE", "P IY P AX L"),
    ("WAS", "W AH Z"), ("WANT", "W AA N T"), ("WORLD", "W ER L D"), ("EVERY", "EH V R IY"), ("READY", "R EH D IY"), ("NEW", "N UW"), ("PARTY", "P AA R T IY"), ("BODY", "B AA D IY"),
    ("EVERYBODY", "EH V R IY B AA D IY"), ("FEEL", "F IY L"), ("REAL", "R IY L"), ("TODAY", "T AX D EY"), ("TONIGHT", "T AX N AY T"), ("OF", "AX V"), ("HI", "HH AY"), ("HELLO", "HH EH L OW"),
];

const LETTERS: [&str; 26] = [
    "EY", "B IY", "S IY", "D IY", "IY", "EH F", "JH IY", "EY CH", "AY", "JH EY", "K EY", "EH L", "EH M", "EH N", "OW", "P IY", "K Y UW", "AA R", "EH S", "T IY", "Y UW", "V IY", "D AH B AX L Y UW",
    "EH K S", "W AY", "Z IY",
];

struct Rule {
    left: &'static str,
    mid: &'static str,
    right: &'static str,
    out: &'static str,
}

fn rules() -> &'static Vec<Vec<Rule>> {
    static R: std::sync::OnceLock<Vec<Vec<Rule>>> = std::sync::OnceLock::new();
    R.get_or_init(|| {
        let mut by: Vec<Vec<Rule>> = (0..26).map(|_| vec![]).collect();
        for r in RULES {
            let (pat, out) = r.split_once('=').unwrap_or((r, ""));
            let mut it = pat.splitn(3, '|');
            let (left, mid, right) = (it.next().unwrap_or(""), it.next().unwrap_or(""), it.next().unwrap_or(""));
            if let Some(c) = mid.bytes().next().filter(|c| c.is_ascii_uppercase()) {
                by[(c - b'A') as usize].push(Rule { left, mid, right, out });
            }
        }
        by
    })
}

fn vowel(c: u8) -> bool {
    matches!(c, b'A' | b'E' | b'I' | b'O' | b'U' | b'Y')
}
fn cons(c: u8) -> bool {
    c.is_ascii_uppercase() && !vowel(c)
}

fn fits_left(t: &[u8], at: usize, pat: &str) -> bool {
    let mut pos = at as isize - 1;
    let get = |p: isize| if p < 0 { b' ' } else { t[p as usize] };
    for c in pat.bytes().rev() {
        match c {
            b'#' => {
                if !vowel(get(pos)) {
                    return false;
                }
                while vowel(get(pos)) {
                    pos -= 1;
                }
            }
            b':' => {
                while cons(get(pos)) {
                    pos -= 1;
                }
            }
            b'^' => {
                if !cons(get(pos)) {
                    return false;
                }
                pos -= 1;
            }
            b'.' => {
                if !matches!(get(pos), b'B' | b'D' | b'V' | b'G' | b'J' | b'L' | b'M' | b'N' | b'R' | b'W' | b'Z') {
                    return false;
                }
                pos -= 1;
            }
            b'+' => {
                if !matches!(get(pos), b'E' | b'I' | b'Y') {
                    return false;
                }
                pos -= 1;
            }
            lit => {
                if get(pos) != lit {
                    return false;
                }
                pos -= 1;
            }
        }
    }
    true
}

fn fits_right(t: &[u8], at: usize, pat: &str) -> bool {
    let mut pos = at;
    let get = |p: usize| t.get(p).copied().unwrap_or(b' ');
    for c in pat.bytes() {
        match c {
            b'#' => {
                if !vowel(get(pos)) {
                    return false;
                }
                while vowel(get(pos)) {
                    pos += 1;
                }
            }
            b':' => {
                while cons(get(pos)) {
                    pos += 1;
                }
            }
            b'^' => {
                if !cons(get(pos)) {
                    return false;
                }
                pos += 1;
            }
            b'.' => {
                if !matches!(get(pos), b'B' | b'D' | b'V' | b'G' | b'J' | b'L' | b'M' | b'N' | b'R' | b'W' | b'Z') {
                    return false;
                }
                pos += 1;
            }
            b'+' => {
                if !matches!(get(pos), b'E' | b'I' | b'Y') {
                    return false;
                }
                pos += 1;
            }
            b'%' => {
                let rest = &t[pos.min(t.len())..];
                let n = if rest.starts_with(b"ING") || rest.starts_with(b"ELY") {
                    3
                } else if rest.starts_with(b"ER") || rest.starts_with(b"ES") || rest.starts_with(b"ED") {
                    2
                } else if rest.starts_with(b"E") {
                    1
                } else {
                    return false;
                };
                pos += n;
            }
            lit => {
                if get(pos) != lit {
                    return false;
                }
                pos += 1;
            }
        }
    }
    true
}

fn number_words(n: u64, out: &mut Vec<String>) {
    const ONES: [&str; 20] = ["ZERO", "ONE", "TWO", "THREE", "FOUR", "FIVE", "SIX", "SEVEN", "EIGHT", "NINE", "TEN", "ELEVEN", "TWELVE", "THIRTEEN", "FOURTEEN", "FIFTEEN", "SIXTEEN", "SEVENTEEN", "EIGHTEEN", "NINETEEN"];
    const TENS: [&str; 10] = ["", "", "TWENTY", "THIRTY", "FORTY", "FIFTY", "SIXTY", "SEVENTY", "EIGHTY", "NINETY"];
    if n >= 1_000_000 {
        number_words(n / 1_000_000, out);
        out.push("MILLION".into());
        if n % 1_000_000 > 0 {
            number_words(n % 1_000_000, out);
        }
    } else if n >= 1000 {
        number_words(n / 1000, out);
        out.push("THOUSAND".into());
        if n % 1000 > 0 {
            number_words(n % 1000, out);
        }
    } else if n >= 100 {
        out.push(ONES[(n / 100) as usize].into());
        out.push("HUNDRED".into());
        if n % 100 > 0 {
            number_words(n % 100, out);
        }
    } else if n >= 20 {
        out.push(TENS[(n / 10) as usize].into());
        if n % 10 > 0 {
            out.push(ONES[(n % 10) as usize].into());
        }
    } else {
        out.push(ONES[n as usize].into());
    }
}

fn word_sounds(word: &str, out: &mut Vec<&'static str>) {
    let push = |s: &'static str, out: &mut Vec<&'static str>| out.extend(s.split(' ').filter(|x| !x.is_empty()));
    if let Some((_, s)) = WORDS.iter().find(|(w, _)| *w == word) {
        return push(s, out);
    }
    let letters: Vec<u8> = word.bytes().filter(|c| c.is_ascii_uppercase()).collect();
    // one letter, or a word nobody could say: spelled out
    if letters.len() == 1 && !matches!(letters[0], b'A' | b'I') || (letters.len() > 1 && letters.len() <= 4 && !letters.iter().any(|&c| vowel(c))) {
        for c in letters {
            push(LETTERS[(c - b'A') as usize], out);
        }
        return;
    }
    let mut t = vec![b' '];
    t.extend(word.bytes());
    t.push(b' ');
    let by = rules();
    let mut i = 1;
    while i + 1 < t.len() {
        let c = t[i];
        if !c.is_ascii_uppercase() {
            i += 1;
            continue;
        }
        let hit = by[(c - b'A') as usize].iter().find(|r| t[i..].starts_with(r.mid.as_bytes()) && fits_left(&t, i, r.left) && fits_right(&t, i + r.mid.len(), r.right));
        match hit {
            Some(r) => {
                push(r.out, out);
                i += r.mid.len();
            }
            None => i += 1,
        }
    }
}

/// The sounds of a line of words, in order. "," is a short rest, "." a long one, "?" a rest after a rise, and
/// "/" the edge between two words.
pub fn sounds(text: &str) -> Vec<&'static str> {
    let up = text.to_ascii_uppercase();
    let mut words: Vec<String> = vec![];
    let mut cur = String::new();
    let flush = |cur: &mut String, words: &mut Vec<String>| {
        if cur.is_empty() {
            return;
        }
        if cur.bytes().all(|c| c.is_ascii_digit()) {
            match cur.parse::<u64>() {
                Ok(n) if cur.len() <= 9 => number_words(n, words),
                _ => {
                    for d in cur.bytes() {
                        number_words((d - b'0') as u64, words);
                    }
                }
            }
        } else {
            words.push(cur.clone());
        }
        cur.clear();
    };
    for ch in up.chars() {
        match ch {
            'A'..='Z' | '\'' => {
                if cur.bytes().next().is_some_and(|c| c.is_ascii_digit()) {
                    flush(&mut cur, &mut words);
                }
                cur.push(ch);
            }
            '0'..='9' => {
                if cur.bytes().next().is_some_and(|c| !c.is_ascii_digit()) {
                    flush(&mut cur, &mut words);
                }
                cur.push(ch);
            }
            ',' | ';' | ':' => {
                flush(&mut cur, &mut words);
                words.push(",".into());
            }
            '.' | '!' => {
                flush(&mut cur, &mut words);
                words.push(".".into());
            }
            '?' => {
                flush(&mut cur, &mut words);
                words.push("?".into());
            }
            _ => flush(&mut cur, &mut words),
        }
    }
    flush(&mut cur, &mut words);
    let mut out = vec![];
    for w in &words {
        match w.as_str() {
            "," => out.push(","),
            "." => out.push("."),
            "?" => out.push("?"),
            w => {
                if !out.is_empty() {
                    out.push("/");
                }
                word_sounds(w.trim_matches('\''), &mut out);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------------------
// sounds to a wave

#[derive(Clone, Copy, Default)]
struct Seg {
    f: [f32; 3],
    /// Voice, breath through the mouth, hiss at the teeth or lips.
    av: f32,
    ah: f32,
    af: f32,
    /// Centre and width of the hiss.
    fc: f32,
    fw: f32,
    ms: f32,
    /// Pitch factor.
    lift: f32,
    /// The mouth jumps to this shape at once (after a closure).
    jump: bool,
}

fn seg(f: [f32; 3], av: f32, ms: f32) -> Seg {
    Seg { f, av, ms, lift: 1.0, fc: 4000.0, fw: 2000.0, ..Seg::default() }
}

/// The pieces a sound is made of. `next` is the mouth shape of the sound that follows, for breath sounds.
fn pieces(ph: &str, next: [f32; 3], stressed: bool, out: &mut Vec<Seg>) {
    let v = |f: [f32; 3], ms: f32, out: &mut Vec<Seg>| out.push(seg(f, 1.0, if stressed { ms * 1.25 } else { ms }));
    let hiss = |voiced: bool, fc: f32, fw: f32, amp: f32, f: [f32; 3], out: &mut Vec<Seg>| out.push(Seg { af: amp, fc, fw, av: if voiced { 0.45 } else { 0.0 }, ..seg(f, 0.0, if voiced { 70.0 } else { 95.0 }) });
    let stop = |voiced: bool, f: [f32; 3], fc: f32, fw: f32, out: &mut Vec<Seg>| {
        out.push(Seg { av: if voiced { 0.12 } else { 0.0 }, ..seg(f, 0.0, if voiced { 45.0 } else { 55.0 }) });
        out.push(Seg { af: 0.5, fc, fw, av: if voiced { 0.3 } else { 0.0 }, ..seg(f, 0.0, 12.0) });
        if !voiced {
            out.push(Seg { ah: 0.35, ..seg(next, 0.0, 28.0) });
        }
    };
    const LAB: [f32; 3] = [300.0, 900.0, 2300.0];
    const ALV: [f32; 3] = [300.0, 1700.0, 2600.0];
    const VEL: [f32; 3] = [300.0, 2200.0, 2500.0];
    match ph {
        "IY" => v([270.0, 2290.0, 3010.0], 105.0, out),
        "IH" => v([390.0, 1990.0, 2550.0], 85.0, out),
        "EH" => v([530.0, 1840.0, 2480.0], 95.0, out),
        "AE" => v([660.0, 1720.0, 2410.0], 115.0, out),
        "AA" => v([730.0, 1090.0, 2440.0], 120.0, out),
        "AH" => v([640.0, 1190.0, 2390.0], 90.0, out),
        "AO" => v([570.0, 840.0, 2410.0], 115.0, out),
        "UH" => v([440.0, 1020.0, 2240.0], 85.0, out),
        "UW" => v([300.0, 870.0, 2240.0], 110.0, out),
        "ER" => v([490.0, 1350.0, 1690.0], 110.0, out),
        "AX" => out.push(seg([500.0, 1500.0, 2500.0], 0.85, 55.0)),
        "EY" => {
            v([480.0, 1950.0, 2500.0], 75.0, out);
            v([330.0, 2200.0, 2800.0], 70.0, out);
        }
        "AY" => {
            v([720.0, 1150.0, 2440.0], 85.0, out);
            v([380.0, 2000.0, 2600.0], 75.0, out);
        }
        "OY" => {
            v([560.0, 850.0, 2410.0], 85.0, out);
            v([380.0, 2000.0, 2600.0], 75.0, out);
        }
        "AW" => {
            v([720.0, 1200.0, 2440.0], 85.0, out);
            v([420.0, 950.0, 2240.0], 75.0, out);
        }
        "OW" => {
            v([540.0, 1050.0, 2400.0], 75.0, out);
            v([380.0, 900.0, 2240.0], 70.0, out);
        }
        "L" => out.push(seg([330.0, 1100.0, 2800.0], 0.8, 60.0)),
        "R" => out.push(seg([330.0, 1100.0, 1450.0], 0.8, 60.0)),
        "W" => out.push(seg([290.0, 650.0, 2150.0], 0.75, 55.0)),
        "Y" => out.push(seg([260.0, 2100.0, 3020.0], 0.75, 50.0)),
        "M" => out.push(Seg { jump: true, ..seg([260.0, 1000.0, 2200.0], 0.5, 65.0) }),
        "N" => out.push(Seg { jump: true, ..seg([260.0, 1500.0, 2500.0], 0.5, 60.0) }),
        "NG" => out.push(Seg { jump: true, ..seg([260.0, 2000.0, 2700.0], 0.5, 70.0) }),
        "S" => hiss(false, 6200.0, 2600.0, 0.55, ALV, out),
        "Z" => hiss(true, 6200.0, 2600.0, 0.35, ALV, out),
        "SH" => hiss(false, 2900.0, 1500.0, 0.6, [300.0, 1900.0, 2500.0], out),
        "ZH" => hiss(true, 2900.0, 1500.0, 0.38, [300.0, 1900.0, 2500.0], out),
        "F" => hiss(false, 7000.0, 6000.0, 0.16, LAB, out),
        "V" => hiss(true, 7000.0, 6000.0, 0.1, LAB, out),
        "TH" => hiss(false, 7500.0, 5000.0, 0.13, ALV, out),
        "DH" => hiss(true, 7500.0, 5000.0, 0.07, ALV, out),
        "HH" => out.push(Seg { ah: 0.5, jump: true, ..seg(next, 0.0, 60.0) }),
        "P" => stop(false, LAB, 900.0, 1500.0, out),
        "B" => stop(true, LAB, 900.0, 1500.0, out),
        "T" => stop(false, ALV, 4800.0, 2500.0, out),
        "D" => stop(true, ALV, 4800.0, 2500.0, out),
        "K" => stop(false, VEL, 2100.0, 1200.0, out),
        "G" => stop(true, VEL, 2100.0, 1200.0, out),
        "CH" => {
            out.push(seg(ALV, 0.0, 50.0));
            out.push(Seg { af: 0.6, fc: 2900.0, fw: 1500.0, ..seg([300.0, 1900.0, 2500.0], 0.0, 70.0) });
        }
        "JH" => {
            out.push(Seg { av: 0.12, ..seg(ALV, 0.0, 40.0) });
            out.push(Seg { af: 0.4, fc: 2900.0, fw: 1500.0, av: 0.4, ..seg([300.0, 1900.0, 2500.0], 0.0, 55.0) });
        }
        "," => out.push(seg(next, 0.0, 180.0)),
        "." | "?" => out.push(seg(next, 0.0, 320.0)),
        _ => {}
    }
}

fn is_vowel_sound(p: &str) -> bool {
    matches!(p, "IY" | "IH" | "EH" | "AE" | "AA" | "AH" | "AO" | "UH" | "UW" | "ER" | "EY" | "AY" | "OY" | "AW" | "OW")
}

struct Reson {
    y1: f32,
    y2: f32,
    a: f32,
    b: f32,
    c: f32,
}

impl Reson {
    fn new() -> Self {
        Self { y1: 0.0, y2: 0.0, a: 1.0, b: 0.0, c: 0.0 }
    }
    fn set(&mut self, f: f32, bw: f32, sr: f32) {
        let f = f.clamp(40.0, sr * 0.45);
        let r = (-std::f32::consts::PI * bw / sr).exp();
        self.c = -r * r;
        self.b = 2.0 * r * (2.0 * std::f32::consts::PI * f / sr).cos();
        self.a = 1.0 - self.b - self.c;
    }
    fn run(&mut self, x: f32) -> f32 {
        let y = self.a * x + self.b * self.y1 + self.c * self.y2;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// What a personality does to the voice: mouth size (resonances scale), breath, pace, steadiness of pitch
/// (0 = dead level), buzz (a narrower pulse), shake.
fn person(k: u8) -> (f32, f32, f32, f32, f32, f32) {
    match k {
        1 => (1.17, 0.05, 1.0, 1.0, 0.0, 0.0),
        2 => (0.92, 0.0, 1.0, 1.0, 0.0, 0.0),
        3 => (1.3, 0.05, 1.0, 1.0, 0.0, 0.0),
        4 => (0.85, 0.0, 0.95, 1.0, 0.0, 0.0),
        5 => (1.15, 0.25, 1.0, 1.0, 0.0, 0.0),
        6 => (1.0, 0.25, 1.0, 1.0, 0.0, 0.0),
        7 => (1.03, 0.0, 1.0, 1.0, 0.3, 0.0),
        8 => (1.25, 0.0, 1.0, 0.6, 0.8, 0.0),
        9 => (1.0, 0.0, 1.0, 0.0, 0.5, 0.0),
        10 => (1.1, 0.0, 1.0, 0.3, 0.6, 0.02),
        11 => (0.8, 0.0, 0.95, 0.8, 0.0, 0.0),
        12 => (1.0, 0.0, 1.4, 1.0, 0.0, 0.0),
        13 => (1.15, 0.1, 0.9, 1.0, 0.0, 0.03),
        14 => (1.4, 0.0, 1.05, 1.0, 0.0, 0.0),
        15 => (0.88, 0.1, 0.9, 0.8, 0.2, 0.0),
        16 => (1.08, 0.0, 1.0, 1.0, 0.4, 0.0),
        17 => (1.03, 0.3, 0.95, 0.8, 0.0, 0.0),
        18 => (1.0, 0.05, 0.8, 1.3, 0.0, 0.04),
        19 => (1.25, 0.05, 1.0, 1.0, 0.0, 0.0),
        _ => (1.0, 0.0, 1.0, 1.0, 0.0, 0.0),
    }
}

/// Say the words. One channel, at `sr` frames a second.
pub fn render(v: &Voice, sr: u32) -> Vec<f32> {
    let phs = sounds(&v.text);
    let (size, breath, pace, steady, buzz, shake) = person(v.personality);
    let srf = sr as f32;
    // the pieces, each with the pitch it is said at
    let mut segs: Vec<Seg> = vec![];
    let n_ph = phs.len().max(1) as f32;
    let mut word_stress_left = true;
    let mut seed = 0x2545_f491u32;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f32 / u32::MAX as f32 * 2.0 - 1.0
    };
    let mut wander = 1.0f32;
    let asks = phs.iter().rposition(|p| *p == "?");
    for (i, ph) in phs.iter().enumerate() {
        if *ph == "/" {
            word_stress_left = true;
            if v.style == 4 {
                wander = 2f32.powf(rnd() * 0.35);
            }
            continue;
        }
        let stressed = word_stress_left && is_vowel_sound(ph);
        if stressed {
            word_stress_left = false;
        }
        // the mouth shape of the next sound that has one of its own
        let mut probe = vec![];
        for nx in phs[i + 1..].iter().take(3) {
            pieces(nx, [500.0, 1500.0, 2500.0], false, &mut probe);
            if !probe.is_empty() {
                break;
            }
        }
        let next = probe.first().map(|s| s.f).unwrap_or([500.0, 1500.0, 2500.0]);
        let from = segs.len();
        pieces(ph, next, stressed, &mut segs);
        let along = i as f32 / n_ph;
        let mut lift = match v.style {
            2 | 3 => 1.0,
            4 => wander,
            s => {
                let wide = if s == 1 { 1.6 } else { 1.0 };
                let mut l = 1.0 + (0.1 - 0.22 * along) * wide;
                if stressed {
                    l *= 1.0 + 0.08 * wide;
                }
                // a question climbs over its last few sounds
                if let Some(q) = asks {
                    if i < q && q - i <= 4 {
                        l *= 1.0 + 0.07 * (5 - (q - i)) as f32;
                    }
                }
                l
            }
        };
        lift = 1.0 + (lift - 1.0) * steady;
        for s in &mut segs[from..] {
            s.lift = lift;
        }
    }
    if segs.is_empty() {
        return vec![];
    }
    let time = 160.0 / (v.rate.clamp(40.0, 400.0) * pace);
    let total: f32 = segs.iter().map(|s| s.ms * time).sum::<f32>() + 60.0;
    let n = (total / 1000.0 * srf) as usize;
    let mut out = Vec::with_capacity(n);

    let mut res = [Reson::new(), Reson::new(), Reson::new(), Reson::new(), Reson::new()];
    let mut hiss = Reson::new();
    let first = segs.iter().find(|s| s.av > 0.5).map(|s| s.f).unwrap_or(segs[0].f);
    let (mut f, mut av, mut ah, mut af, mut fc, mut fw) = (first, 0.0f32, 0.0f32, 0.0f32, 4000.0f32, 2000.0f32);
    let base = v.pitch.clamp(20.0, 800.0);
    let mut f0 = base * segs[0].lift;
    let (mut phase, mut prev_flow, mut prev_out) = (0.0f32, 0.0f32, 0.0f32);
    let (mut si, mut left) = (0usize, segs[0].ms * time / 1000.0 * srf);
    let k_f = 1.0 - (-1.0 / (0.018 * srf)).exp();
    let k_a = 1.0 - (-1.0 / (0.006 * srf)).exp();
    let k_p = 1.0 - (-1.0 / (0.04 * srf)).exp();
    let open = 0.62 - 0.25 * buzz;
    let whisper = v.mode == 2;
    let extra_breath = breath + if v.mode == 1 { 0.3 } else { 0.0 };
    let mut t = 0usize;
    let mut drift = 0.0f32;
    while out.len() < n {
        if left <= 0.0 && si + 1 < segs.len() {
            si += 1;
            left = segs[si].ms * time / 1000.0 * srf;
            if segs[si].jump {
                f = segs[si].f;
            }
        }
        left -= 1.0;
        let s = if left > 0.0 || si + 1 < segs.len() { segs[si] } else { Seg { f, lift: segs[si].lift, fc, fw, ..Seg::default() } };
        for k in 0..3 {
            f[k] += (s.f[k] - f[k]) * k_f;
        }
        av += (s.av - av) * k_a;
        ah += (s.ah - ah) * k_a;
        af += (s.af - af) * k_a;
        fc += (s.fc - fc) * k_a;
        fw += (s.fw - fw) * k_a;
        if t % 32 == 0 {
            drift += (rnd() * shake - drift) * 0.02;
            let sung = if v.style == 3 { 1.0 + 0.012 * (t as f32 / srf * 5.5 * std::f32::consts::TAU).sin() } else { 1.0 };
            let want = base * s.lift * sung * (1.0 + drift * 4.0 + 0.012 * rnd() * steady.min(1.0));
            f0 += (want - f0) * (k_p * 32.0).min(1.0);
            res[0].set(f[0] * size, 70.0, srf);
            res[1].set(f[1] * size, 100.0, srf);
            res[2].set(f[2] * size, 160.0, srf);
            res[3].set(3500.0 * size, 250.0, srf);
            res[4].set(4500.0 * size, 300.0, srf);
            hiss.set(fc, fw, srf);
        }
        t += 1;
        // the pulse of the voice: opens smoothly, shuts fast
        phase += f0 / srf;
        if phase >= 1.0 {
            phase -= 1.0;
        }
        let rise = open * 0.7;
        let flow = if phase < rise {
            0.5 * (1.0 - (std::f32::consts::PI * phase / rise).cos())
        } else if phase < open {
            (std::f32::consts::FRAC_PI_2 * (phase - rise) / (open - rise)).cos()
        } else {
            0.0
        };
        let pulse = (flow - prev_flow) * (srf / 44_100.0) * 14.0;
        prev_flow = flow;
        let noise = rnd();
        let (voice, breathing) = if whisper { (0.0, ah + av * 0.5) } else { (av, ah + av * extra_breath * 0.4) };
        let mut x = pulse * voice + noise * breathing * 0.25;
        for r in &mut res {
            x = r.run(x);
        }
        let y = x + hiss.run(noise) * af * 0.5;
        // the lips let the highs out more easily than the lows
        let o = y - 0.6 * prev_out;
        prev_out = y;
        out.push(o);
    }
    let peak = out.iter().fold(0.0f32, |a, b| a.max(b.abs()));
    if peak > 0.0 {
        let g = 0.8 / peak;
        for s in &mut out {
            *s *= g;
        }
    }
    // a short fade at both ends, so it starts and stops without a tick
    let edge = (srf * 0.004) as usize;
    let len = out.len();
    for i in 0..edge.min(len / 2) {
        let g = i as f32 / edge as f32;
        out[i] *= g;
        out[len - 1 - i] *= g;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preset_is_read_and_written_back() {
        let text = "[Speaker Settings]\r\nStyle=2\r\nPersonality=9\r\nPitch=92\r\nRate=150\r\nVoicingMode=0\r\nText=Type your text here\r\n";
        let v = parse(text.as_bytes()).unwrap();
        assert_eq!((v.style, v.personality, v.pitch, v.rate, v.mode, v.text.as_str()), (2, 9, 92.0, 150.0, 0, "Type your text here"));
        assert_eq!(write(&v), text);
        assert!(parse(b"RIFF....WAVE").is_none());
    }

    #[test]
    fn words_become_sounds() {
        assert_eq!(sounds("the cat").join(" "), "DH AX / K AE T");
        assert_eq!(sounds("ready?").join(" "), "R EH D IY ?");
        assert_eq!(sounds("D_J 12").join(" "), "D IY / JH EY / T W EH L V");
        assert_eq!(sounds("music").join(" "), "M Y UW Z IH K");
    }

    #[test]
    fn speech_has_sound_of_a_sensible_length() {
        let v = Voice { text: "hello world".into(), ..Voice::default() };
        let w = render(&v, 44_100);
        let secs = w.len() as f32 / 44_100.0;
        assert!(secs > 0.5 && secs < 2.5, "{secs}");
        assert!(w.iter().all(|s| s.is_finite()));
        let rms = (w.iter().map(|s| s * s).sum::<f32>() / w.len() as f32).sqrt();
        assert!(rms > 0.03, "{rms}");
        // twice the rate, about half as long
        let fast = render(&Voice { rate: 300.0, ..v }, 44_100);
        assert!((fast.len() as f32) < w.len() as f32 * 0.6);
    }
}
