use std::{
    collections::{HashMap, HashSet},
    env, fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) enum Category {
    Emoji,
    Kaomoji,
    Greek,
    Cyrillic,
    Latin,
    Ipa,
    Hebrew,
    Arabic,
    Kana,
    Math,
    Punctuation,
    Currency,
    Arrows,
    BoxDrawing,
    Blocks,
    Shapes,
    Keyboard,
    SuperscriptsSubscripts,
    Fractions,
    Braille,
    Games,
    Music,
    Units,
    Enclosed,
}

impl Category {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Category::Emoji => "Emoji",
            Category::Kaomoji => "Kaomoji",
            Category::Greek => "Greek Letters",
            Category::Cyrillic => "Cyrillic",
            Category::Latin => "Latin Extended",
            Category::Ipa => "IPA Phonetics",
            Category::Hebrew => "Hebrew",
            Category::Arabic => "Arabic",
            Category::Kana => "Japanese Kana",
            Category::Math => "Math Symbols",
            Category::Punctuation => "Punctuation",
            Category::Currency => "Currency",
            Category::Arrows => "Arrows",
            Category::BoxDrawing => "Box Drawing",
            Category::Blocks => "Blocks",
            Category::Shapes => "Shapes",
            Category::Keyboard => "Keyboard",
            Category::SuperscriptsSubscripts => "Super/Subscript",
            Category::Fractions => "Fractions",
            Category::Braille => "Braille",
            Category::Games => "Games",
            Category::Music => "Music",
            Category::Units => "Units",
            Category::Enclosed => "Enclosed",
        }
    }

    pub(crate) fn icon(self) -> &'static str {
        match self {
            Category::Emoji => "🙂",
            Category::Kaomoji => ":-)",
            Category::Greek => "α",
            Category::Cyrillic => "Ж",
            Category::Latin => "Ł",
            Category::Ipa => "ɸ",
            Category::Hebrew => "א",
            Category::Arabic => "ع",
            Category::Kana => "あ",
            Category::Math => "∑",
            Category::Punctuation => "¶",
            Category::Currency => "€",
            Category::Arrows => "→",
            Category::BoxDrawing => "┼",
            Category::Blocks => "█",
            Category::Shapes => "◆",
            Category::Keyboard => "⌘",
            Category::SuperscriptsSubscripts => "²",
            Category::Fractions => "½",
            Category::Braille => "⠿",
            Category::Games => "♞",
            Category::Music => "♫",
            Category::Units => "℃",
            Category::Enclosed => "①",
        }
    }

    fn from_file_name(file_name: &str) -> Option<(Self, Option<EmojiGroup>)> {
        let name = file_name.to_lowercase();

        if name == "kaomoji.csv" || name.contains("kaomoji") {
            Some((Category::Kaomoji, None))
        } else if name.starts_with("emojis_")
            && name != "emojis_symbols.csv"
            && name != "emojis_component.csv"
        {
            Some((Category::Emoji, EmojiGroup::from_file_name(&name)))
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) enum EmojiGroup {
    SmileysEmotion,
    AnimalsNature,
    FoodDrink,
    TravelPlaces,
    Activities,
    PeopleBody,
    Objects,
    Flags,
}

impl EmojiGroup {
    pub(crate) const ALL: [EmojiGroup; 8] = [
        EmojiGroup::SmileysEmotion,
        EmojiGroup::AnimalsNature,
        EmojiGroup::FoodDrink,
        EmojiGroup::TravelPlaces,
        EmojiGroup::Activities,
        EmojiGroup::PeopleBody,
        EmojiGroup::Objects,
        EmojiGroup::Flags,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            EmojiGroup::SmileysEmotion => "Smileys",
            EmojiGroup::AnimalsNature => "Animals",
            EmojiGroup::FoodDrink => "Food",
            EmojiGroup::TravelPlaces => "Travel",
            EmojiGroup::Activities => "Activities",
            EmojiGroup::PeopleBody => "People",
            EmojiGroup::Objects => "Objects",
            EmojiGroup::Flags => "Flags",
        }
    }

    pub(crate) fn icon(self) -> &'static str {
        match self {
            EmojiGroup::SmileysEmotion => "😀",
            EmojiGroup::AnimalsNature => "🐱",
            EmojiGroup::FoodDrink => "🍔",
            EmojiGroup::TravelPlaces => "🚍",
            EmojiGroup::Activities => "⚽",
            EmojiGroup::PeopleBody => "👕",
            EmojiGroup::Objects => "🎵",
            EmojiGroup::Flags => "🚩",
        }
    }

    fn from_file_name(file_name: &str) -> Option<Self> {
        match file_name {
            "emojis_smileys_emotion.csv" => Some(EmojiGroup::SmileysEmotion),
            "emojis_animals_nature.csv" => Some(EmojiGroup::AnimalsNature),
            "emojis_food_drink.csv" => Some(EmojiGroup::FoodDrink),
            "emojis_travel_places.csv" => Some(EmojiGroup::TravelPlaces),
            "emojis_activities.csv" => Some(EmojiGroup::Activities),
            "emojis_people_body.csv" => Some(EmojiGroup::PeopleBody),
            "emojis_objects.csv" => Some(EmojiGroup::Objects),
            "emojis_flags.csv" => Some(EmojiGroup::Flags),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub(crate) ch: String,
    pub(crate) desc: String,
    pub(crate) category: Category,
    pub(crate) emoji_group: Option<EmojiGroup>,
    pub(crate) search_text: String,
}

impl Entry {
    fn new(ch: impl Into<String>, desc: impl Into<String>, category: Category) -> Self {
        Self::with_group(ch, desc, category, None)
    }

    fn with_group(
        ch: impl Into<String>,
        desc: impl Into<String>,
        category: Category,
        emoji_group: Option<EmojiGroup>,
    ) -> Self {
        let ch = ch.into();
        let desc = desc.into();
        let search_text = format!("{} {}", ch, desc).to_lowercase();

        Self {
            ch,
            desc,
            category,
            emoji_group,
            search_text,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct StoredEntry {
    ch: String,
    desc: String,
    category: Category,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    emoji_group: Option<EmojiGroup>,
}

impl From<&Entry> for StoredEntry {
    fn from(entry: &Entry) -> Self {
        Self {
            ch: entry.ch.clone(),
            desc: entry.desc.clone(),
            category: entry.category,
            emoji_group: entry.emoji_group,
        }
    }
}

impl From<StoredEntry> for Entry {
    fn from(entry: StoredEntry) -> Self {
        Entry::with_group(entry.ch, entry.desc, entry.category, entry.emoji_group)
    }
}

#[derive(Clone, Debug)]
pub(crate) enum DataSource {
    Rofimoji(PathBuf),
    BuiltIn,
}

pub(crate) fn load_entries() -> (Vec<Entry>, DataSource) {
    for data_dir in candidate_data_dirs() {
        if let Ok(mut entries) = load_entries_from_dir(&data_dir)
            && !entries.is_empty()
        {
            entries.extend(curated_symbol_entries());
            ensure_custom_kaomoji(&mut entries);
            return (dedup_entries(entries), DataSource::Rofimoji(data_dir));
        }
    }

    let mut entries = built_in_entries();
    entries.extend(curated_symbol_entries());
    (dedup_entries(entries), DataSource::BuiltIn)
}

pub(crate) fn recent_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("symbolis").join("recent.json"))
}

pub(crate) fn load_recent(path: &Path) -> Option<Vec<Entry>> {
    let content = fs::read_to_string(path).ok()?;
    let entries: Vec<StoredEntry> = serde_json::from_str(&content).ok()?;
    Some(entries.into_iter().map(Entry::from).collect())
}

fn candidate_data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(path) = env::var("ROFIMOJI_DATA_DIR") {
        dirs.push(PathBuf::from(path));
    }

    for base in ["/usr/lib", "/usr/local/lib"] {
        let Ok(entries) = fs::read_dir(base) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };

            if !name.starts_with("python3") {
                continue;
            }

            dirs.push(path.join("site-packages/picker/data"));
            dirs.push(path.join("dist-packages/picker/data"));
        }
    }

    dirs.push(PathBuf::from("/usr/share/rofimoji/data"));
    dirs.push(PathBuf::from("/usr/local/share/rofimoji/data"));

    dedup_paths(dirs)
}

fn dedup_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

fn load_entries_from_dir(dir: &Path) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    let mut files = Vec::new();

    for file in fs::read_dir(dir)? {
        files.push(file?.path());
    }

    files.sort_by(|a, b| {
        file_priority(a)
            .cmp(&file_priority(b))
            .then_with(|| a.file_name().cmp(&b.file_name()))
    });

    for path in files {
        if !path.is_file() {
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some((category, emoji_group)) = Category::from_file_name(file_name) else {
            continue;
        };

        let content = fs::read_to_string(&path)?;
        entries.extend(parse_entries(&content, category, emoji_group));
    }

    Ok(dedup_entries(entries)
        .into_iter()
        .filter(should_include_entry)
        .collect())
}

fn file_priority(path: &Path) -> usize {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    [
        "emojis_smileys_emotion.csv",
        "emojis_people_body.csv",
        "emojis_animals_nature.csv",
        "emojis_food_drink.csv",
        "emojis_activities.csv",
        "emojis_travel_places.csv",
        "emojis_objects.csv",
        "emojis_flags.csv",
        "kaomoji.csv",
    ]
    .iter()
    .position(|candidate| *candidate == name)
    .unwrap_or(usize::MAX)
}

fn should_include_entry(entry: &Entry) -> bool {
    match entry.category {
        Category::Emoji => looks_like_emoji(&entry.ch),
        _ => true,
    }
}

fn ensure_custom_kaomoji(entries: &mut Vec<Entry>) {
    if let Some(entry) = entries
        .iter_mut()
        .find(|entry| entry.category == Category::Kaomoji && entry.ch == "¯\\_(ツ)_/¯")
    {
        let ch = entry.ch.clone();
        entry.desc = "shrug".to_owned();
        entry.search_text = format!("{} {}", ch, entry.desc).to_lowercase();
    } else {
        entries.push(Entry::new("¯\\_(ツ)_/¯", "shrug", Category::Kaomoji));
    }
}

fn curated_greek_entries() -> Vec<Entry> {
    [
        ("α", "alpha"),
        ("β", "beta"),
        ("γ", "gamma"),
        ("δ", "delta"),
        ("ε", "epsilon"),
        ("ζ", "zeta"),
        ("η", "eta"),
        ("θ", "theta"),
        ("ι", "iota"),
        ("κ", "kappa"),
        ("λ", "lambda"),
        ("μ", "mu"),
        ("ν", "nu"),
        ("ξ", "xi"),
        ("ο", "omicron"),
        ("π", "pi"),
        ("ρ", "rho"),
        ("σ", "sigma"),
        ("ς", "final sigma"),
        ("τ", "tau"),
        ("υ", "upsilon"),
        ("φ", "phi"),
        ("χ", "chi"),
        ("ψ", "psi"),
        ("ω", "omega"),
        ("Α", "Alpha"),
        ("Β", "Beta"),
        ("Γ", "Gamma"),
        ("Δ", "Delta"),
        ("Ε", "Epsilon"),
        ("Ζ", "Zeta"),
        ("Η", "Eta"),
        ("Θ", "Theta"),
        ("Ι", "Iota"),
        ("Κ", "Kappa"),
        ("Λ", "Lambda"),
        ("Μ", "Mu"),
        ("Ν", "Nu"),
        ("Ξ", "Xi"),
        ("Ο", "Omicron"),
        ("Π", "Pi"),
        ("Ρ", "Rho"),
        ("Σ", "Sigma"),
        ("Τ", "Tau"),
        ("Υ", "Upsilon"),
        ("Φ", "Phi"),
        ("Χ", "Chi"),
        ("Ψ", "Psi"),
        ("Ω", "Omega"),
    ]
    .into_iter()
    .map(|(ch, desc)| Entry::new(ch, desc, Category::Greek))
    .collect()
}

fn curated_math_entries() -> Vec<Entry> {
    [
        ("±", "plus minus"),
        ("×", "multiply"),
        ("÷", "divide"),
        ("=", "equals"),
        ("≠", "not equal"),
        ("≈", "approximately equal"),
        ("≡", "identical to"),
        ("<", "less than"),
        (">", "greater than"),
        ("≤", "less or equal"),
        ("≥", "greater or equal"),
        ("∞", "infinity"),
        ("√", "square root"),
        ("∛", "cube root"),
        ("∜", "fourth root"),
        ("∑", "sum"),
        ("∏", "product"),
        ("∫", "integral"),
        ("∬", "double integral"),
        ("∮", "contour integral"),
        ("∂", "partial derivative"),
        ("∇", "nabla"),
        ("∆", "increment"),
        ("∈", "element of"),
        ("∉", "not element of"),
        ("∋", "contains as member"),
        ("∅", "empty set"),
        ("∪", "union"),
        ("∩", "intersection"),
        ("⊂", "proper subset"),
        ("⊃", "proper superset"),
        ("⊆", "subset or equal"),
        ("⊇", "superset or equal"),
        ("∧", "logical and"),
        ("∨", "logical or"),
        ("¬", "logical not"),
        ("∀", "for all"),
        ("∃", "there exists"),
        ("∄", "does not exist"),
        ("⇒", "implies"),
        ("⇔", "if and only if"),
        ("→", "right arrow"),
        ("←", "left arrow"),
        ("↔", "left right arrow"),
        ("⊕", "circled plus"),
        ("⊗", "circled times"),
        ("⊥", "perpendicular"),
        ("∥", "parallel"),
        ("∴", "therefore"),
        ("∵", "because"),
        ("∝", "proportional to"),
        ("‰", "per mille"),
        ("′", "prime"),
        ("″", "double prime"),
        ("°", "degree"),
    ]
    .into_iter()
    .map(|(ch, desc)| Entry::new(ch, desc, Category::Math))
    .collect()
}

fn curated_symbol_entries() -> Vec<Entry> {
    let mut entries = Vec::new();
    entries.extend(curated_greek_entries());
    entries.extend(curated_math_entries());
    entries.extend(entries_for(
        Category::Cyrillic,
        &[
            ("А", "Cyrillic capital A"),
            ("Б", "Cyrillic capital Be"),
            ("В", "Cyrillic capital Ve"),
            ("Г", "Cyrillic capital Ghe"),
            ("Д", "Cyrillic capital De"),
            ("Е", "Cyrillic capital Ie"),
            ("Ё", "Cyrillic capital Io"),
            ("Ж", "Cyrillic capital Zhe"),
            ("З", "Cyrillic capital Ze"),
            ("И", "Cyrillic capital I"),
            ("Й", "Cyrillic capital Short I"),
            ("К", "Cyrillic capital Ka"),
            ("Л", "Cyrillic capital El"),
            ("М", "Cyrillic capital Em"),
            ("Н", "Cyrillic capital En"),
            ("О", "Cyrillic capital O"),
            ("П", "Cyrillic capital Pe"),
            ("Р", "Cyrillic capital Er"),
            ("С", "Cyrillic capital Es"),
            ("Т", "Cyrillic capital Te"),
            ("У", "Cyrillic capital U"),
            ("Ф", "Cyrillic capital Ef"),
            ("Х", "Cyrillic capital Ha"),
            ("Ц", "Cyrillic capital Tse"),
            ("Ч", "Cyrillic capital Che"),
            ("Ш", "Cyrillic capital Sha"),
            ("Щ", "Cyrillic capital Shcha"),
            ("Ъ", "Cyrillic hard sign"),
            ("Ы", "Cyrillic Yery"),
            ("Ь", "Cyrillic soft sign"),
            ("Э", "Cyrillic E"),
            ("Ю", "Cyrillic Yu"),
            ("Я", "Cyrillic Ya"),
            ("а", "Cyrillic small a"),
            ("б", "Cyrillic small be"),
            ("в", "Cyrillic small ve"),
            ("г", "Cyrillic small ghe"),
            ("д", "Cyrillic small de"),
            ("е", "Cyrillic small ie"),
            ("ё", "Cyrillic small io"),
            ("ж", "Cyrillic small zhe"),
            ("з", "Cyrillic small ze"),
            ("и", "Cyrillic small i"),
            ("й", "Cyrillic small short i"),
            ("к", "Cyrillic small ka"),
            ("л", "Cyrillic small el"),
            ("м", "Cyrillic small em"),
            ("н", "Cyrillic small en"),
            ("о", "Cyrillic small o"),
            ("п", "Cyrillic small pe"),
            ("р", "Cyrillic small er"),
            ("с", "Cyrillic small es"),
            ("т", "Cyrillic small te"),
            ("у", "Cyrillic small u"),
            ("ф", "Cyrillic small ef"),
            ("х", "Cyrillic small ha"),
            ("ц", "Cyrillic small tse"),
            ("ч", "Cyrillic small che"),
            ("ш", "Cyrillic small sha"),
            ("щ", "Cyrillic small shcha"),
            ("э", "Cyrillic small e"),
            ("ю", "Cyrillic small yu"),
            ("я", "Cyrillic small ya"),
            ("і", "Ukrainian i"),
            ("ї", "Ukrainian yi"),
            ("є", "Ukrainian ye"),
            ("ґ", "Ukrainian ghe with upturn"),
            ("Ў", "Belarusian short u"),
            ("ў", "Belarusian short u"),
        ],
    ));
    entries.extend(entries_for(
        Category::Latin,
        &[
            ("À", "A grave"),
            ("Á", "A acute"),
            ("Â", "A circumflex"),
            ("Ã", "A tilde"),
            ("Ä", "A diaeresis"),
            ("Å", "A ring"),
            ("Æ", "AE ligature"),
            ("Ç", "C cedilla"),
            ("È", "E grave"),
            ("É", "E acute"),
            ("Ê", "E circumflex"),
            ("Ë", "E diaeresis"),
            ("Ì", "I grave"),
            ("Í", "I acute"),
            ("Î", "I circumflex"),
            ("Ï", "I diaeresis"),
            ("Ñ", "N tilde"),
            ("Ò", "O grave"),
            ("Ó", "O acute"),
            ("Ô", "O circumflex"),
            ("Õ", "O tilde"),
            ("Ö", "O diaeresis"),
            ("Ø", "O stroke"),
            ("Ù", "U grave"),
            ("Ú", "U acute"),
            ("Û", "U circumflex"),
            ("Ü", "U diaeresis"),
            ("Ý", "Y acute"),
            ("Þ", "Thorn"),
            ("ß", "Sharp s"),
            ("à", "a grave"),
            ("á", "a acute"),
            ("â", "a circumflex"),
            ("ã", "a tilde"),
            ("ä", "a diaeresis"),
            ("å", "a ring"),
            ("æ", "ae ligature"),
            ("ç", "c cedilla"),
            ("è", "e grave"),
            ("é", "e acute"),
            ("ê", "e circumflex"),
            ("ë", "e diaeresis"),
            ("ñ", "n tilde"),
            ("ö", "o diaeresis"),
            ("ø", "o stroke"),
            ("ü", "u diaeresis"),
            ("þ", "thorn"),
            ("ÿ", "y diaeresis"),
            ("Ā", "A macron"),
            ("ā", "a macron"),
            ("Č", "C caron"),
            ("č", "c caron"),
            ("Đ", "D stroke"),
            ("đ", "d stroke"),
            ("Ė", "E dot above"),
            ("ė", "e dot above"),
            ("Ł", "L stroke"),
            ("ł", "l stroke"),
            ("Œ", "OE ligature"),
            ("œ", "oe ligature"),
            ("Š", "S caron"),
            ("š", "s caron"),
            ("Ž", "Z caron"),
            ("ž", "z caron"),
        ],
    ));
    entries.extend(entries_for(
        Category::Ipa,
        &[
            ("ɑ", "open back unrounded vowel"),
            ("ɐ", "turned a"),
            ("æ", "near-open front unrounded vowel"),
            ("ɒ", "open back rounded vowel"),
            ("ə", "schwa"),
            ("ɚ", "r-colored schwa"),
            ("ɛ", "open-mid front unrounded vowel"),
            ("ɜ", "open-mid central unrounded vowel"),
            ("ɞ", "open-mid central rounded vowel"),
            ("ɪ", "near-close near-front vowel"),
            ("ɨ", "close central unrounded vowel"),
            ("ʊ", "near-close near-back rounded vowel"),
            ("ʌ", "open-mid back unrounded vowel"),
            ("ɔ", "open-mid back rounded vowel"),
            ("ø", "close-mid front rounded vowel"),
            ("ɯ", "close back unrounded vowel"),
            ("β", "voiced bilabial fricative"),
            ("θ", "voiceless dental fricative"),
            ("ð", "voiced dental fricative"),
            ("ʃ", "voiceless postalveolar fricative"),
            ("ʒ", "voiced postalveolar fricative"),
            ("ŋ", "eng"),
            ("ɲ", "palatal nasal"),
            ("ʔ", "glottal stop"),
            ("ɾ", "alveolar tap"),
            ("ɹ", "alveolar approximant"),
            ("ʁ", "uvular fricative"),
            ("χ", "voiceless uvular fricative"),
            ("ʂ", "retroflex fricative"),
            ("ʐ", "voiced retroflex fricative"),
            ("ʈ", "retroflex stop"),
            ("ɖ", "voiced retroflex stop"),
            ("ˈ", "primary stress"),
            ("ˌ", "secondary stress"),
            ("ː", "long mark"),
            ("̃", "nasalization"),
        ],
    ));
    entries.extend(entries_for(
        Category::Hebrew,
        &[
            ("א", "aleph"),
            ("ב", "bet"),
            ("ג", "gimel"),
            ("ד", "dalet"),
            ("ה", "he"),
            ("ו", "vav"),
            ("ז", "zayin"),
            ("ח", "het"),
            ("ט", "tet"),
            ("י", "yod"),
            ("כ", "kaf"),
            ("ך", "final kaf"),
            ("ל", "lamed"),
            ("מ", "mem"),
            ("ם", "final mem"),
            ("נ", "nun"),
            ("ן", "final nun"),
            ("ס", "samekh"),
            ("ע", "ayin"),
            ("פ", "pe"),
            ("ף", "final pe"),
            ("צ", "tsadi"),
            ("ץ", "final tsadi"),
            ("ק", "qof"),
            ("ר", "resh"),
            ("ש", "shin"),
            ("ת", "tav"),
        ],
    ));
    entries.extend(entries_for(
        Category::Arabic,
        &[
            ("ا", "alef"),
            ("ب", "beh"),
            ("ت", "teh"),
            ("ث", "theh"),
            ("ج", "jeem"),
            ("ح", "hah"),
            ("خ", "khah"),
            ("د", "dal"),
            ("ذ", "thal"),
            ("ر", "reh"),
            ("ز", "zain"),
            ("س", "seen"),
            ("ش", "sheen"),
            ("ص", "sad"),
            ("ض", "dad"),
            ("ط", "tah"),
            ("ظ", "zah"),
            ("ع", "ain"),
            ("غ", "ghain"),
            ("ف", "feh"),
            ("ق", "qaf"),
            ("ك", "kaf"),
            ("ل", "lam"),
            ("م", "meem"),
            ("ن", "noon"),
            ("ه", "heh"),
            ("و", "waw"),
            ("ي", "yeh"),
            ("ء", "hamza"),
            ("ة", "teh marbuta"),
            ("ى", "alef maksura"),
        ],
    ));
    entries.extend(entries_for(
        Category::Kana,
        &[
            ("あ", "hiragana a"),
            ("い", "hiragana i"),
            ("う", "hiragana u"),
            ("え", "hiragana e"),
            ("お", "hiragana o"),
            ("か", "hiragana ka"),
            ("き", "hiragana ki"),
            ("く", "hiragana ku"),
            ("け", "hiragana ke"),
            ("こ", "hiragana ko"),
            ("さ", "hiragana sa"),
            ("し", "hiragana shi"),
            ("す", "hiragana su"),
            ("せ", "hiragana se"),
            ("そ", "hiragana so"),
            ("た", "hiragana ta"),
            ("ち", "hiragana chi"),
            ("つ", "hiragana tsu"),
            ("て", "hiragana te"),
            ("と", "hiragana to"),
            ("な", "hiragana na"),
            ("に", "hiragana ni"),
            ("ぬ", "hiragana nu"),
            ("ね", "hiragana ne"),
            ("の", "hiragana no"),
            ("ア", "katakana a"),
            ("イ", "katakana i"),
            ("ウ", "katakana u"),
            ("エ", "katakana e"),
            ("オ", "katakana o"),
            ("カ", "katakana ka"),
            ("キ", "katakana ki"),
            ("ク", "katakana ku"),
            ("ケ", "katakana ke"),
            ("コ", "katakana ko"),
            ("サ", "katakana sa"),
            ("シ", "katakana shi"),
            ("ス", "katakana su"),
            ("セ", "katakana se"),
            ("ソ", "katakana so"),
            ("ン", "katakana n"),
            ("ー", "prolonged sound mark"),
        ],
    ));
    entries.extend(entries_for(
        Category::Punctuation,
        &[
            ("“", "left double quote"),
            ("”", "right double quote"),
            ("‘", "left single quote"),
            ("’", "right single quote"),
            ("‚", "single low quote"),
            ("„", "double low quote"),
            ("‹", "single left angle quote"),
            ("›", "single right angle quote"),
            ("«", "left guillemet"),
            ("»", "right guillemet"),
            ("–", "en dash"),
            ("—", "em dash"),
            ("―", "horizontal bar"),
            ("…", "ellipsis"),
            ("•", "bullet"),
            ("·", "middle dot"),
            ("‣", "triangular bullet"),
            ("⁃", "hyphen bullet"),
            ("§", "section sign"),
            ("¶", "pilcrow"),
            ("†", "dagger"),
            ("‡", "double dagger"),
            ("※", "reference mark"),
            ("‼", "double exclamation"),
            ("⁇", "double question"),
            ("⁈", "question exclamation"),
            ("⁉", "exclamation question"),
            ("№", "numero sign"),
            ("⁂", "asterism"),
            ("〃", "ditto mark"),
        ],
    ));
    entries.extend(entries_for(
        Category::Currency,
        &[
            ("$", "dollar"),
            ("¢", "cent"),
            ("£", "pound"),
            ("¤", "currency sign"),
            ("¥", "yen"),
            ("€", "euro"),
            ("₽", "ruble"),
            ("₿", "bitcoin"),
            ("₩", "won"),
            ("₹", "rupee"),
            ("₴", "hryvnia"),
            ("₺", "lira"),
            ("₫", "dong"),
            ("₪", "new shekel"),
            ("₦", "naira"),
            ("₱", "peso"),
            ("₲", "guarani"),
            ("₡", "colon"),
            ("₭", "kip"),
            ("₮", "tugrik"),
            ("₵", "cedi"),
            ("₸", "tenge"),
            ("₼", "manat"),
            ("₾", "lari"),
        ],
    ));
    entries.extend(entries_for(
        Category::Arrows,
        &[
            ("←", "left arrow"),
            ("↑", "up arrow"),
            ("→", "right arrow"),
            ("↓", "down arrow"),
            ("↔", "left right arrow"),
            ("↕", "up down arrow"),
            ("↖", "north west arrow"),
            ("↗", "north east arrow"),
            ("↘", "south east arrow"),
            ("↙", "south west arrow"),
            ("↩", "left hook arrow"),
            ("↪", "right hook arrow"),
            ("↫", "left loop arrow"),
            ("↬", "right loop arrow"),
            ("↭", "left right wave arrow"),
            ("↯", "down zigzag arrow"),
            ("⇐", "left double arrow"),
            ("⇑", "up double arrow"),
            ("⇒", "right double arrow"),
            ("⇓", "down double arrow"),
            ("⇔", "left right double arrow"),
            ("⇕", "up down double arrow"),
            ("⇠", "left dashed arrow"),
            ("⇢", "right dashed arrow"),
            ("⇤", "left bar arrow"),
            ("⇥", "right bar arrow"),
            ("⟵", "long left arrow"),
            ("⟶", "long right arrow"),
            ("⟷", "long left right arrow"),
            ("⟸", "long left double arrow"),
            ("⟹", "long right double arrow"),
            ("⟺", "long left right double arrow"),
        ],
    ));
    entries.extend(entries_for(
        Category::BoxDrawing,
        &[
            ("─", "box light horizontal"),
            ("│", "box light vertical"),
            ("┌", "box light down right"),
            ("┐", "box light down left"),
            ("└", "box light up right"),
            ("┘", "box light up left"),
            ("├", "box light vertical right"),
            ("┤", "box light vertical left"),
            ("┬", "box light down horizontal"),
            ("┴", "box light up horizontal"),
            ("┼", "box light cross"),
            ("━", "box heavy horizontal"),
            ("┃", "box heavy vertical"),
            ("┏", "box heavy down right"),
            ("┓", "box heavy down left"),
            ("┗", "box heavy up right"),
            ("┛", "box heavy up left"),
            ("┣", "box heavy vertical right"),
            ("┫", "box heavy vertical left"),
            ("┳", "box heavy down horizontal"),
            ("┻", "box heavy up horizontal"),
            ("╋", "box heavy cross"),
            ("═", "box double horizontal"),
            ("║", "box double vertical"),
            ("╔", "box double down right"),
            ("╗", "box double down left"),
            ("╚", "box double up right"),
            ("╝", "box double up left"),
            ("╠", "box double vertical right"),
            ("╣", "box double vertical left"),
            ("╦", "box double down horizontal"),
            ("╩", "box double up horizontal"),
            ("╬", "box double cross"),
        ],
    ));
    entries.extend(entries_for(
        Category::Blocks,
        &[
            ("█", "full block"),
            ("▓", "dark shade"),
            ("▒", "medium shade"),
            ("░", "light shade"),
            ("▀", "upper half block"),
            ("▄", "lower half block"),
            ("▌", "left half block"),
            ("▐", "right half block"),
            ("▁", "lower one eighth block"),
            ("▂", "lower one quarter block"),
            ("▃", "lower three eighths block"),
            ("▄", "lower half block"),
            ("▅", "lower five eighths block"),
            ("▆", "lower three quarters block"),
            ("▇", "lower seven eighths block"),
            ("▉", "left seven eighths block"),
            ("▊", "left three quarters block"),
            ("▋", "left five eighths block"),
            ("▌", "left half block"),
            ("▍", "left three eighths block"),
            ("▎", "left one quarter block"),
            ("▏", "left one eighth block"),
        ],
    ));
    entries.extend(entries_for(
        Category::Shapes,
        &[
            ("○", "white circle"),
            ("●", "black circle"),
            ("◌", "dotted circle"),
            ("◎", "bullseye"),
            ("◯", "large circle"),
            ("□", "white square"),
            ("■", "black square"),
            ("▢", "white square rounded"),
            ("▣", "square with black center"),
            ("◆", "black diamond"),
            ("◇", "white diamond"),
            ("◈", "diamond with center"),
            ("▲", "black up triangle"),
            ("△", "white up triangle"),
            ("▼", "black down triangle"),
            ("▽", "white down triangle"),
            ("◀", "black left triangle"),
            ("▶", "black right triangle"),
            ("★", "black star"),
            ("☆", "white star"),
            ("✦", "four pointed star"),
            ("✧", "white four pointed star"),
            ("✶", "six pointed star"),
            ("✷", "eight pointed star"),
        ],
    ));
    entries.extend(entries_for(
        Category::Keyboard,
        &[
            ("⌘", "command"),
            ("⌥", "option"),
            ("⌃", "control"),
            ("⇧", "shift"),
            ("⇪", "caps lock"),
            ("⎋", "escape"),
            ("⏎", "return"),
            ("⌫", "delete backward"),
            ("⌦", "delete forward"),
            ("⇥", "tab"),
            ("⇤", "back tab"),
            ("␣", "space"),
            ("⌤", "enter key"),
            ("⌧", "clear key"),
            ("⎀", "insert"),
            ("⎙", "print screen"),
            ("⎗", "previous page"),
            ("⎘", "next page"),
            ("⏏", "eject"),
            ("⏻", "power"),
            ("⏼", "power on off"),
            ("⏽", "power sleep"),
        ],
    ));
    entries.extend(entries_for(
        Category::SuperscriptsSubscripts,
        &[
            ("⁰", "superscript zero"),
            ("¹", "superscript one"),
            ("²", "superscript two"),
            ("³", "superscript three"),
            ("⁴", "superscript four"),
            ("⁵", "superscript five"),
            ("⁶", "superscript six"),
            ("⁷", "superscript seven"),
            ("⁸", "superscript eight"),
            ("⁹", "superscript nine"),
            ("⁺", "superscript plus"),
            ("⁻", "superscript minus"),
            ("⁼", "superscript equals"),
            ("⁽", "superscript left parenthesis"),
            ("⁾", "superscript right parenthesis"),
            ("₀", "subscript zero"),
            ("₁", "subscript one"),
            ("₂", "subscript two"),
            ("₃", "subscript three"),
            ("₄", "subscript four"),
            ("₅", "subscript five"),
            ("₆", "subscript six"),
            ("₇", "subscript seven"),
            ("₈", "subscript eight"),
            ("₉", "subscript nine"),
            ("₊", "subscript plus"),
            ("₋", "subscript minus"),
            ("₌", "subscript equals"),
            ("₍", "subscript left parenthesis"),
            ("₎", "subscript right parenthesis"),
            ("ₐ", "subscript a"),
            ("ₑ", "subscript e"),
            ("ₕ", "subscript h"),
            ("ᵢ", "subscript i"),
            ("ⱼ", "subscript j"),
            ("ₖ", "subscript k"),
            ("ₗ", "subscript l"),
            ("ₘ", "subscript m"),
            ("ₙ", "subscript n"),
            ("ₒ", "subscript o"),
            ("ₚ", "subscript p"),
            ("ᵣ", "subscript r"),
            ("ₛ", "subscript s"),
            ("ₜ", "subscript t"),
            ("ᵤ", "subscript u"),
            ("ᵥ", "subscript v"),
            ("ₓ", "subscript x"),
        ],
    ));
    entries.extend(entries_for(
        Category::Fractions,
        &[
            ("¼", "one quarter"),
            ("½", "one half"),
            ("¾", "three quarters"),
            ("⅐", "one seventh"),
            ("⅑", "one ninth"),
            ("⅒", "one tenth"),
            ("⅓", "one third"),
            ("⅔", "two thirds"),
            ("⅕", "one fifth"),
            ("⅖", "two fifths"),
            ("⅗", "three fifths"),
            ("⅘", "four fifths"),
            ("⅙", "one sixth"),
            ("⅚", "five sixths"),
            ("⅛", "one eighth"),
            ("⅜", "three eighths"),
            ("⅝", "five eighths"),
            ("⅞", "seven eighths"),
            ("Ⅰ", "roman numeral one"),
            ("Ⅱ", "roman numeral two"),
            ("Ⅲ", "roman numeral three"),
            ("Ⅳ", "roman numeral four"),
            ("Ⅴ", "roman numeral five"),
            ("Ⅵ", "roman numeral six"),
            ("Ⅶ", "roman numeral seven"),
            ("Ⅷ", "roman numeral eight"),
            ("Ⅸ", "roman numeral nine"),
            ("Ⅹ", "roman numeral ten"),
            ("Ⅼ", "roman numeral fifty"),
            ("Ⅽ", "roman numeral one hundred"),
            ("Ⅾ", "roman numeral five hundred"),
            ("Ⅿ", "roman numeral one thousand"),
        ],
    ));
    entries.extend(entries_for(
        Category::Braille,
        &[
            ("⠀", "braille blank"),
            ("⠁", "braille dots 1"),
            ("⠂", "braille dots 2"),
            ("⠃", "braille dots 12"),
            ("⠄", "braille dots 3"),
            ("⠅", "braille dots 13"),
            ("⠆", "braille dots 23"),
            ("⠇", "braille dots 123"),
            ("⠈", "braille dots 4"),
            ("⠉", "braille dots 14"),
            ("⠊", "braille dots 24"),
            ("⠋", "braille dots 124"),
            ("⠌", "braille dots 34"),
            ("⠍", "braille dots 134"),
            ("⠎", "braille dots 234"),
            ("⠏", "braille dots 1234"),
            ("⠐", "braille dots 5"),
            ("⠑", "braille dots 15"),
            ("⠒", "braille dots 25"),
            ("⠓", "braille dots 125"),
            ("⠔", "braille dots 35"),
            ("⠕", "braille dots 135"),
            ("⠖", "braille dots 235"),
            ("⠗", "braille dots 1235"),
            ("⠘", "braille dots 45"),
            ("⠙", "braille dots 145"),
            ("⠚", "braille dots 245"),
            ("⠛", "braille dots 1245"),
            ("⠜", "braille dots 345"),
            ("⠝", "braille dots 1345"),
            ("⠞", "braille dots 2345"),
            ("⠟", "braille dots 12345"),
            ("⠿", "braille dots 123456"),
            ("⡿", "braille high fill"),
            ("⣿", "braille full block"),
        ],
    ));
    entries.extend(entries_for(
        Category::Games,
        &[
            ("♔", "white chess king"),
            ("♕", "white chess queen"),
            ("♖", "white chess rook"),
            ("♗", "white chess bishop"),
            ("♘", "white chess knight"),
            ("♙", "white chess pawn"),
            ("♚", "black chess king"),
            ("♛", "black chess queen"),
            ("♜", "black chess rook"),
            ("♝", "black chess bishop"),
            ("♞", "black chess knight"),
            ("♟", "black chess pawn"),
            ("♠", "spade suit"),
            ("♥", "heart suit"),
            ("♦", "diamond suit"),
            ("♣", "club suit"),
            ("♡", "white heart suit"),
            ("♢", "white diamond suit"),
            ("♤", "white spade suit"),
            ("♧", "white club suit"),
            ("⚀", "die face one"),
            ("⚁", "die face two"),
            ("⚂", "die face three"),
            ("⚃", "die face four"),
            ("⚄", "die face five"),
            ("⚅", "die face six"),
        ],
    ));
    entries.extend(entries_for(
        Category::Music,
        &[
            ("♪", "eighth note"),
            ("♫", "beamed eighth notes"),
            ("♬", "beamed sixteenth notes"),
            ("♩", "quarter note"),
            ("♭", "flat"),
            ("♮", "natural"),
            ("♯", "sharp"),
            ("𝄞", "g clef"),
            ("𝄢", "f clef"),
            ("𝄡", "c clef"),
            ("𝄪", "double sharp"),
            ("𝄫", "double flat"),
            ("𝄐", "fermata"),
            ("𝄑", "fermata below"),
            ("𝄆", "left repeat sign"),
            ("𝄇", "right repeat sign"),
            ("𝄽", "quarter rest"),
            ("𝄾", "eighth rest"),
        ],
    ));
    entries.extend(entries_for(
        Category::Units,
        &[
            ("°", "degree"),
            ("℃", "degree Celsius"),
            ("℉", "degree Fahrenheit"),
            ("Å", "angstrom"),
            ("µ", "micro sign"),
            ("Ω", "ohm"),
            ("℧", "mho"),
            ("ℓ", "liter"),
            ("№", "number"),
            ("℮", "estimated sign"),
            ("℅", "care of"),
            ("℆", "cada una"),
            ("℞", "prescription take"),
            ("℥", "ounce"),
            ("℔", "pound sign"),
            ("℺", "rotated capital Q"),
            ("‰", "per mille"),
            ("‱", "per ten thousand"),
            ("′", "prime"),
            ("″", "double prime"),
            ("‴", "triple prime"),
        ],
    ));
    entries.extend(entries_for(
        Category::Enclosed,
        &[
            ("①", "circled digit one"),
            ("②", "circled digit two"),
            ("③", "circled digit three"),
            ("④", "circled digit four"),
            ("⑤", "circled digit five"),
            ("⑥", "circled digit six"),
            ("⑦", "circled digit seven"),
            ("⑧", "circled digit eight"),
            ("⑨", "circled digit nine"),
            ("⑩", "circled number ten"),
            ("⓪", "circled digit zero"),
            ("Ⓐ", "circled capital A"),
            ("Ⓑ", "circled capital B"),
            ("Ⓒ", "circled capital C"),
            ("Ⓓ", "circled capital D"),
            ("Ⓔ", "circled capital E"),
            ("Ⓕ", "circled capital F"),
            ("Ⓖ", "circled capital G"),
            ("ⓐ", "circled small a"),
            ("ⓑ", "circled small b"),
            ("ⓒ", "circled small c"),
            ("ⓘ", "circled information source"),
            ("ⓧ", "circled small x"),
            ("🄯", "copyleft symbol"),
            ("🄫", "circled C"),
            ("🄬", "circled R"),
        ],
    ));
    entries
}

fn entries_for(category: Category, data: &[(&str, &str)]) -> Vec<Entry> {
    data.iter()
        .map(|(ch, desc)| Entry::new(*ch, *desc, category))
        .collect()
}

fn is_symbolish_emoji(ch: char) -> bool {
    matches!(
        ch,
        '\u{1f000}'..='\u{1faff}'
            | '\u{2600}'..='\u{27bf}'
            | '\u{2300}'..='\u{23ff}'
            | '\u{2b00}'..='\u{2bff}'
    )
}

fn looks_like_emoji(value: &str) -> bool {
    value.chars().any(is_symbolish_emoji)
}

fn parse_entries(content: &str, category: Category, emoji_group: Option<EmojiGroup>) -> Vec<Entry> {
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim_end();
            if line.trim().is_empty() || line.trim_start().starts_with('#') {
                return None;
            }

            let (ch, desc) = parse_entry_line(line);

            if ch.is_empty() {
                return None;
            }

            Some(Entry::with_group(
                ch,
                clean_description(desc),
                category,
                emoji_group,
            ))
        })
        .collect()
}

fn parse_entry_line(line: &str) -> (&str, &str) {
    if let Some((ch, desc)) = line.split_once('\t') {
        return (ch.trim(), desc.trim());
    }

    if let Some(rest) = line.strip_prefix(' ') {
        return (" ", rest.trim());
    }

    line.split_once(' ')
        .map(|(ch, desc)| (ch.trim(), desc.trim()))
        .unwrap_or((line.trim(), ""))
}

fn clean_description(desc: &str) -> String {
    desc.replace("<small>", "")
        .replace("</small>", "")
        .trim()
        .to_owned()
}

fn dedup_entries(entries: Vec<Entry>) -> Vec<Entry> {
    let mut seen = HashMap::new();
    let mut result = Vec::new();

    for entry in entries {
        if seen
            .insert((entry.category, entry.ch.clone()), ())
            .is_none()
        {
            result.push(entry);
        }
    }

    result
}

fn built_in_entries() -> Vec<Entry> {
    let data: &[(Category, &[(&str, &str)])] = &[
        (
            Category::Emoji,
            &[
                ("😀", "grinning face"),
                ("😄", "smiling face"),
                ("😂", "face with tears of joy"),
                ("🥰", "smiling face with hearts"),
                ("😎", "smiling face with sunglasses"),
                ("🤔", "thinking face"),
                ("👍", "thumbs up"),
                ("🙏", "folded hands"),
                ("🔥", "fire"),
                ("✨", "sparkles"),
                ("🎉", "party popper"),
                ("❤️", "red heart"),
            ],
        ),
        (
            Category::Kaomoji,
            &[
                ("(^_^)", "happy"),
                ("(>_<)", "frustrated"),
                ("(-_-)", "unamused"),
                ("(o_O)", "confused"),
                ("\\(^o^)/", "excited"),
                ("(T_T)", "crying"),
                ("(._.)", "looking down"),
                ("(╯°□°)╯", "table flip start"),
                ("¯\\_(ツ)_/¯", "shrug"),
                ("(づ｡◕‿‿◕｡)づ", "hug"),
                ("(•_•)", "serious"),
                ("(ﾉ◕ヮ◕)ﾉ*:･ﾟ✧", "magic"),
            ],
        ),
        (
            Category::Greek,
            &[
                ("α", "alpha"),
                ("β", "beta"),
                ("γ", "gamma"),
                ("δ", "delta"),
                ("ε", "epsilon"),
                ("θ", "theta"),
                ("λ", "lambda"),
                ("μ", "mu"),
                ("π", "pi"),
                ("ρ", "rho"),
                ("σ", "sigma"),
                ("φ", "phi"),
                ("ψ", "psi"),
                ("ω", "omega"),
            ],
        ),
        (
            Category::Math,
            &[
                ("±", "plus minus"),
                ("×", "multiplication"),
                ("÷", "division"),
                ("≈", "approximately equal"),
                ("≠", "not equal"),
                ("≤", "less than or equal"),
                ("≥", "greater than or equal"),
                ("∞", "infinity"),
                ("√", "square root"),
                ("∑", "summation"),
                ("∏", "product"),
                ("∫", "integral"),
                ("∂", "partial derivative"),
                ("∇", "nabla"),
                ("∈", "element of"),
                ("∅", "empty set"),
            ],
        ),
    ];

    data.iter()
        .flat_map(|(category, entries)| {
            entries.iter().map(move |(ch, desc)| {
                let group = if *category == Category::Emoji {
                    Some(EmojiGroup::SmileysEmotion)
                } else {
                    None
                };
                Entry::with_group(*ch, *desc, *category, group)
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_symbol_and_clean_description() {
        let entries = parse_entries(
            "😀 grinning face <small>(face, grin)</small>\n",
            Category::Emoji,
            Some(EmojiGroup::SmileysEmotion),
        );

        assert_eq!(entries[0].ch, "😀");
        assert_eq!(entries[0].desc, "grinning face (face, grin)");
        assert_eq!(entries[0].emoji_group, Some(EmojiGroup::SmileysEmotion));
    }

    #[test]
    fn parses_space_symbol() {
        let entries = parse_entries("  Space\n", Category::Math, None);

        assert_eq!(entries[0].ch, " ");
        assert_eq!(entries[0].desc, "Space");
    }

    #[test]
    fn categorizes_rofimoji_files() {
        assert_eq!(
            Category::from_file_name("emojis_smileys_emotion.csv"),
            Some((Category::Emoji, Some(EmojiGroup::SmileysEmotion)))
        );
        assert_eq!(
            Category::from_file_name("kaomoji.csv"),
            Some((Category::Kaomoji, None))
        );
        assert_eq!(Category::from_file_name("emojis_symbols.csv"), None);
        assert_eq!(Category::from_file_name("emoticons.csv"), None);
        assert_eq!(Category::from_file_name("greek_and_coptic.csv"), None);
        assert_eq!(Category::from_file_name("mathematical_operators.csv"), None);
    }

    #[test]
    fn curated_categories_are_focused() {
        let greek = curated_greek_entries();
        let math = curated_math_entries();

        assert!(greek.iter().any(|entry| entry.ch == "α"));
        assert!(greek.iter().any(|entry| entry.ch == "Ω"));
        assert!(math.iter().any(|entry| entry.ch == "∑"));
        assert!(math.iter().any(|entry| entry.ch == "⇒"));
        assert!(math.iter().all(|entry| !entry.desc.contains("Arabic")));
    }

    #[test]
    fn curated_symbol_categories_cover_common_sets() {
        let entries = curated_symbol_entries();

        assert!(
            entries
                .iter()
                .any(|entry| entry.category == Category::Cyrillic && entry.ch == "Ж")
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.category == Category::Arabic && entry.ch == "ع")
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.category == Category::Currency && entry.ch == "₿")
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.category == Category::BoxDrawing && entry.ch == "┼")
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.category == Category::Keyboard && entry.ch == "⌘")
        );

        let entries = dedup_entries(entries);
        assert!(
            entries
                .iter()
                .any(|entry| entry.category == Category::Math && entry.ch == "→")
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.category == Category::Arrows && entry.ch == "→")
        );
    }

    #[test]
    fn custom_kaomoji_adds_shrug_once() {
        let mut entries = vec![
            Entry::new("(^_^)", "happy", Category::Kaomoji),
            Entry::new("¯\\_(ツ)_/¯", "whatever", Category::Kaomoji),
        ];
        ensure_custom_kaomoji(&mut entries);
        ensure_custom_kaomoji(&mut entries);

        let shrugs = entries
            .iter()
            .filter(|entry| entry.ch == "¯\\_(ツ)_/¯" && entry.desc == "shrug")
            .count();
        assert_eq!(shrugs, 1);
    }
}
