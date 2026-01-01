use anyhow::{Result, anyhow};
use glob::glob;
use logos::Logos;
use serde::{Deserialize, Serialize};
use std::io::{BufReader, Write};
use std::ops::Range;
use std::path::PathBuf;

#[derive(Default, Debug, Clone, PartialEq)]
pub enum LexingError {
    #[default]
    Other,
}

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"\r\n")]
pub enum Token {
    // #[token("\r\n")]
    // Newline,
    #[regex(r"<[A-Z\+\-\d]{3}", priority = 6)]
    LbCode,
    #[regex(r"\d{3,4}", priority = 7)]
    NumCode,
    #[token("<FAC0000")]
    NormalWidth,
    #[token("<FAC0001")]
    SueSmile,
    #[token("<FAC0002")]
    SueFrown,
    #[token("<FAC0003")]
    SueAngry,
    #[token("<FAC0004")]
    SueHurt,
    #[token("<FAC0005")]
    BalrogNormal,
    #[token("<FAC0006")]
    TorokoNormal,
    #[token("<FAC0007")]
    King,
    #[token("<FAC0008")]
    TorokoAngry,
    #[token("<FAC0009")]
    Jack,
    #[token("<FAC0010")]
    Kazuma,
    #[token("<FAC0011")]
    TorokoRage,
    #[token("<FAC0012")]
    Igor,
    #[token("<FAC0013")]
    Jenka,
    #[token("<FAC0014")]
    BalrogSmile,
    #[token("<FAC0015")]
    MiseryNormal,
    #[token("<FAC0016")]
    MiserySmile,
    #[token("<FAC0017")]
    BoosterHurt,
    #[token("<FAC0018")]
    BoosterNormal,
    #[token("<FAC0019")]
    CurlySmile,
    #[token("<FAC0020")]
    CurlyFrown,
    #[token("<FAC0021")]
    Doctor,
    #[token("<FAC0022")]
    Momorin,
    #[token("<FAC0023")]
    BalrogHurt,
    #[token("<FAC0024")]
    BrokenRobot,
    #[token("<FAC0025")]
    CurlyUnknown,
    #[token("<FAC0026")]
    MiseryAngry,
    #[token("<FAC0027")]
    HumanSue,
    #[token("<FAC0028")]
    Itoh,
    #[token("<FAC0029")]
    Ballos,
    #[token("<MSG")]
    Message,
    #[token("<NOD")]
    Nod,
    #[token("<CLR")]
    Clear,
    #[token("<END")]
    End,
    #[token("#")]
    Pound,
    #[token(":")]
    Colon,
    #[regex(r#"[\d]{3}|[\-a-zA-Z.\!?=\*'" ][a-zA-Z,.!?;\d\+\-\'"= \*\r\n]*(?:<NUM0000)?"#, |lex| lex.slice().to_owned())]
    Text(String),
    #[regex(r".", priority=1, callback = |lex| lex.slice().to_owned())]
    Other(String),
}

impl Token {
    pub fn is_face(&self) -> bool {
        matches!(
            self,
            Token::NormalWidth
                | Token::SueSmile
                | Token::SueFrown
                | Token::SueAngry
                | Token::SueHurt
                | Token::BalrogNormal
                | Token::TorokoNormal
                | Token::King
                | Token::TorokoAngry
                | Token::Jack
                | Token::Kazuma
                | Token::TorokoRage
                | Token::Igor
                | Token::Jenka
                | Token::BalrogSmile
                | Token::MiseryNormal
                | Token::MiserySmile
                | Token::BoosterHurt
                | Token::BoosterNormal
                | Token::CurlySmile
                | Token::CurlyFrown
                | Token::Doctor
                | Token::Momorin
                | Token::BalrogHurt
                | Token::BrokenRobot
                | Token::CurlyUnknown
                | Token::MiseryAngry
                | Token::HumanSue
                | Token::Itoh
                | Token::Ballos
        )
    }
}

pub fn tsc_decode(b: Vec<u8>) -> Vec<u8> {
    let enc_idx = b.len() / 2;
    let enc = b[enc_idx];
    b.iter()
        .enumerate()
        .map(|(i, c)| match i == enc_idx {
            false => c.wrapping_sub(enc),
            true => *c,
        })
        .collect()
}

pub fn tsc_encode(s: String) -> Vec<u8> {
    let b: Vec<u8> = s.into();
    let enc_idx = b.len() / 2;
    let enc = b[enc_idx];
    b.iter()
        .enumerate()
        .map(|(i, c)| match i == enc_idx {
            false => c.wrapping_add(enc),
            true => *c,
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Speech {
    character: String,
    text: Vec<(String, Range<usize>)>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileData {
    dialogues: Vec<Vec<Speech>>,
    original: String,
    path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct DialogueData {
    game_data_root: PathBuf,
    files: Vec<FileData>,
}

impl FileData {
    pub fn reconstruct(&self) -> String {
        let mut str = String::new();
        let mut last_range_end = 0;
        for speech in self.dialogues.iter().flatten() {
            for (text, range) in &speech.text {
                str += &self.original[last_range_end..range.start];
                str += text;
                last_range_end = range.end;
            }
        }
        str += &self.original[last_range_end..self.original.len()];
        str
    }
}

fn dialogues_from_tsc(text: &str) -> Vec<Vec<Speech>> {
    let mut lex = Token::lexer(text);
    let mut character = String::new();
    let mut speech: Vec<(String, Range<usize>)> = vec![];
    let mut dialogue: Vec<Speech> = vec![];
    let mut dialogues: Vec<Vec<Speech>> = vec![];
    while let Some(Ok(token)) = lex.next() {
        if matches!(token, Token::Message) {
            if !speech.is_empty() {
                dialogue.push(Speech {
                    character: character.clone(),
                    text: speech.clone(),
                });
            }
            if !dialogue.is_empty() {
                dialogues.push(dialogue.clone());
            }
            dialogue.clear();
            speech.clear();
        }
        if matches!(token, Token::Message | Token::NormalWidth) {
            character = "NP".to_string();
        }
        if token.is_face() {
            if !speech.is_empty() {
                // println!("{:?}\n{}", &speech, &text[span_start..span_end]);
                dialogue.push(Speech {
                    character: character.clone(),
                    text: speech.clone(),
                });
            }
            speech.clear();
            character = format!("{token:?}");
        } else if let Token::Text(s) = token {
            speech.push((s, lex.span()));
        }
    }
    dialogues
}

#[derive(Debug)]
struct AppArgs {
    game_data: Option<PathBuf>,
    translation_file: Option<PathBuf>,
    output_dir: Option<PathBuf>,
}

fn dump(data_dir: PathBuf, output: PathBuf) -> Result<()> {
    let mut files: Vec<FileData> = vec![];
    let pattern = data_dir.join("**/*.tsc");

    for path in (glob(
        pattern
            .to_str()
            .ok_or(anyhow!("couldn't stringify pattern"))?,
    )?)
    .flatten()
    {
        let bytes = tsc_decode(std::fs::read(&path)?);
        let text = String::from_utf8_lossy(&bytes);
        let dialogues = dialogues_from_tsc(&text);
        if !dialogues.is_empty() {
            let data = FileData {
                dialogues,
                original: text.to_string(),
                path,
            };
            files.push(data);
        }
    }

    let dialogue = DialogueData {
        game_data_root: data_dir,
        files,
    };

    let j = serde_json::to_string(&dialogue)?;
    let mut outfile = std::fs::File::create(&output)?;
    outfile.write_all(j.as_bytes())?;
    Ok(())
}

fn write(translation_file: PathBuf, output_dir: PathBuf) -> Result<()> {
    let file = std::fs::File::open(translation_file)?;
    let reader = BufReader::new(file);
    let dd: DialogueData = serde_json::from_reader(reader)?;
    let dir = output_dir;
    std::fs::create_dir_all(&dir)?;
    for fd in dd.files {
        let p = dir.join(fd.path.strip_prefix(&dd.game_data_root)?);
        let s = fd.reconstruct();
        let enc = tsc_encode(s);
        std::fs::create_dir_all(
            p.parent()
                .ok_or(anyhow!("couldn't create parent directory"))?,
        )?;
        let mut outfile = std::fs::File::create(&p)?;
        outfile.write_all(&enc)?;
        println!("Wrote {p:?}");
    }
    Ok(())
}

// from https://github.com/RazrFalcon/pico-args/blob/master/examples/app.rs
fn parse_path(s: &std::ffi::OsStr) -> Result<std::path::PathBuf, &'static str> {
    Ok(s.into())
}

fn help() -> Result<()> {
    Err(anyhow!(
        "Usage: doukutsu-extractor [OPTIONS] COMMAND

OPTIONS
  --translation_file FILE     Path to the JSON translation file (required).
  --game_data DIRECTORY       Path to the game-data folder (required for
                              the “dump” command).
  --output_dir DIRECTORY      Path to the output folder (required for the
                              “write” command).

COMMANDS
  dump                        Extract translatable text from the game data
                              into the translation file.
  write                       Re-build the game files from the translation file
                              and write them to the output directory.

EXAMPLES
  doukutsu-extractor --translation_file texts.json --game_data ./CaveStory/data dump
  doukutsu-extractor --translation_file texts.json --output_dir ./out write"
    ))
}

fn main() -> Result<()> {
    let mut pargs = pico_args::Arguments::from_env();

    let args = AppArgs {
        game_data: pargs.opt_value_from_os_str("--game_data", parse_path)?,
        translation_file: pargs.opt_value_from_os_str("--translation_file", parse_path)?,
        output_dir: pargs.opt_value_from_os_str("--output_dir", parse_path)?,
    };

    let subcommand = pargs.subcommand();

    match subcommand {
        Ok(Some(sc)) => match sc.as_str() {
            "dump" => dump(
                args.game_data
                    .ok_or(anyhow!("missing `--game_data DIRECTORY`"))?,
                args.translation_file
                    .ok_or(anyhow!("missing --translation_file FILE.json"))?,
            ),
            "write" => write(
                args.translation_file
                    .ok_or(anyhow!("missing --translation_file FILE.json"))?,
                args.output_dir.ok_or(anyhow!("missing --output_dir"))?,
            ),
            _ => help(),
        },
        _ => help(),
    }

    // let reconstructed = files[2].reconstruct();
    // println!("PARSED: {:#?}", files[2].dialogues);
    // println!("RECONSTRUCTED:\n {:#?}", reconstructed);
    // println!("ORIGINAL:\n {:#?}", files[2].original)
}
