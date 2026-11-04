use crate::Result;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_support::{DecodeStringError, ReadStringError, ReadStringsExt, write_i32_str};
use std::io::{Read, Write};
use std::result::Result as StdResult;

/// The error code of an AI script that failed to compile, as the game's AI rule parser reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, num_enum::IntoPrimitive, num_enum::FromPrimitive)]
#[repr(u32)]
#[non_exhaustive]
pub enum AiErrorCode {
    /// A `defconst` redefines a constant that already exists.
    ConstantAlreadyDefined = 0,
    /// The AI file could not be opened.
    FileOpenFailed = 1,
    /// The AI file could not be read.
    FileReadFailed = 2,
    /// An identifier is not valid.
    InvalidIdentifier = 3,
    /// A keyword is not valid.
    InvalidKeyword = 4,
    /// A preprocessor directive (`#load-if-defined` and the like) is not valid.
    InvalidPreprocessorDirective = 5,
    /// One of the parser's fixed-size lists is full.
    ListFull = 6,
    /// A rule lacks the `=>` between its conditions and its actions.
    MissingArrow = 7,
    /// A closing parenthesis is missing.
    MissingClosingParenthesis = 8,
    /// A string lacks its closing quote.
    MissingClosingQuote = 9,
    /// A conditional block lacks its `#end-if`.
    MissingEndIf = 10,
    /// A directive that loads a file lacks the file name.
    MissingFileName = 11,
    /// An identifier is missing.
    MissingIdentifier = 12,
    /// A keyword is missing.
    MissingKeyword = 13,
    /// A rule lacks its left-hand side (the conditions).
    MissingLhs = 14,
    /// An opening parenthesis is missing.
    MissingOpeningParenthesis = 15,
    /// A preprocessor directive lacks its symbol.
    MissingPreprocessorSymbol = 16,
    /// A rule lacks its right-hand side (the actions).
    MissingRhs = 17,
    /// The script contains no rules.
    NoRules = 18,
    /// Conditional blocks are nested too deeply.
    PreprocessorNestingTooDeep = 19,
    /// A rule exceeds the parser's length limit.
    RuleTooLong = 20,
    /// The parser's string table is full.
    StringTableFull = 21,
    /// An error the game does not describe.
    UndocumentedError = 22,
    /// An `#else` without a matching `#if`.
    UnexpectedElse = 23,
    /// An `#end-if` without a matching `#if`.
    UnexpectedEndIf = 24,
    /// An unexpected internal error.
    UnexpectedError = 25,
    /// The file ends in the middle of a construct.
    UnexpectedEof = 26,
    /// A code the game's parser does not name, kept to be written back.
    #[num_enum(catch_all)]
    Other(u32),
}

/// The compile error the scenario's AI script had when the scenario was saved.
#[derive(Debug, Clone)]
pub struct AiErrorInfo {
    filename: String,
    line_number: i32,
    description: String,
    error_code: AiErrorCode,
}

fn parse_bytes(bytes: &[u8]) -> StdResult<String, ReadStringError> {
    let mut bytes = bytes.to_vec();
    if let Some(end) = bytes.iter().position(|&byte| byte == 0) {
        bytes.truncate(end);
    }
    if bytes.is_empty() {
        Ok("<empty>".to_string())
    } else {
        String::from_utf8(bytes).map_err(|_| ReadStringError::DecodeStringError(DecodeStringError))
    }
}

impl AiErrorInfo {
    /// Read AI error information from an input stream.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        // TODO support non UTF8 encoding
        let mut filename_bytes = [0; 257];
        input.read_exact(&mut filename_bytes)?;
        let line_number = input.read_i32::<LE>()?;
        let mut description_bytes = [0; 128];
        input.read_exact(&mut description_bytes)?;
        let error_code = AiErrorCode::from(input.read_u32::<LE>()?);

        let filename = parse_bytes(&filename_bytes)?;
        let description = parse_bytes(&description_bytes)?;

        Ok(Self {
            filename,
            line_number,
            description,
            error_code,
        })
    }

    /// Write AI error information to an output stream.
    pub fn write_to(&self, mut output: impl Write) -> Result<()> {
        // TODO support non UTF8 encoding
        let mut filename_bytes = [0; 257];
        filename_bytes[..self.filename.len()].copy_from_slice(self.filename.as_bytes());
        output.write_all(&filename_bytes)?;

        output.write_i32::<LE>(self.line_number)?;

        let mut description_bytes = [0; 128];
        description_bytes[..self.description.len()].copy_from_slice(self.description.as_bytes());
        output.write_all(&description_bytes)?;

        output.write_u32::<LE>(self.error_code.into())?;

        Ok(())
    }
}

/// An AI script embedded in the scenario: its file name and its source text.
#[derive(Debug, Clone)]
pub struct AiFile {
    filename: String,
    content: String,
}

impl AiFile {
    /// Read an embedded AI file from an input stream.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let filename = input
            .read_u32_length_prefixed_str()?
            .ok_or(crate::Error::MissingAiFileNameError)?;
        let content = input
            .read_u32_length_prefixed_str()?
            .ok_or(crate::Error::EmptyAiFileError)?;

        Ok(Self { filename, content })
    }

    /// Write this embedded AI file to an output stream.
    pub fn write_to(&self, mut output: impl Write) -> Result<()> {
        write_i32_str(&mut output, &self.filename)?;
        write_i32_str(&mut output, &self.content)?;
        Ok(())
    }
}

/// The scenario's embedded AI scripts and the last AI compile error.
#[derive(Debug, Default, Clone)]
pub struct AiInfo {
    error: Option<AiErrorInfo>,
    files: Vec<AiFile>,
}

impl AiInfo {
    /// Read the AI scripts block; `None` if it holds neither scripts nor an error.
    pub fn read_from(mut input: impl Read) -> Result<Option<Self>> {
        let has_ai_files = input.read_u32::<LE>()? != 0;
        let has_error = input.read_u32::<LE>()? != 0;

        if !has_error && !has_ai_files {
            return Ok(None);
        }

        let error = if has_error {
            Some(AiErrorInfo::read_from(&mut input)?)
        } else {
            None
        };

        let num_ai_files = input.read_u32::<LE>()?;
        let mut files = vec![];
        for _ in 0..num_ai_files {
            files.push(AiFile::read_from(&mut input)?);
        }

        Ok(Some(Self { error, files }))
    }

    /// Write the AI scripts block.
    // The format stores the number of AI files as a u32.
    #[allow(clippy::cast_possible_truncation)]
    pub fn write_to(&self, mut output: impl Write) -> Result<()> {
        output.write_u32::<LE>(u32::from(!self.files.is_empty()))?;

        if let Some(error) = &self.error {
            output.write_u32::<LE>(1)?;
            error.write_to(&mut output)?;
        } else {
            output.write_u32::<LE>(0)?;
        }

        if !self.files.is_empty() {
            output.write_u32::<LE>(self.files.len() as u32)?;
            for file in &self.files {
                file.write_to(&mut output)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An AI error's fixed-size record: the file name and description up to their first NUL
    /// (an empty one reads as `<empty>`), the line and the error code; written back the same.
    #[test]
    fn an_ai_error_record_reads_and_writes_its_fields() {
        let mut bytes = vec![0u8; 257 + 4 + 128 + 4];
        bytes[..5].copy_from_slice(b"a.per");
        bytes[257..261].copy_from_slice(&12i32.to_le_bytes());
        bytes[261..264].copy_from_slice(b"bad");
        bytes[389..].copy_from_slice(&7u32.to_le_bytes());
        let info = AiErrorInfo::read_from(bytes.as_slice()).unwrap();
        assert_eq!(
            (
                info.filename.as_str(),
                info.line_number,
                info.description.as_str(),
                info.error_code
            ),
            ("a.per", 12, "bad", AiErrorCode::MissingArrow)
        );
        let mut out = vec![];
        info.write_to(&mut out).unwrap();
        assert_eq!(out, bytes);
        bytes[..5].fill(0);
        let unnamed = AiErrorInfo::read_from(bytes.as_slice()).unwrap();
        assert_eq!(unnamed.filename, "<empty>");
        // a code the parser does not name reads as it is and writes back
        bytes[..5].copy_from_slice(b"a.per");
        bytes[389..].copy_from_slice(&27u32.to_le_bytes());
        let other = AiErrorInfo::read_from(bytes.as_slice()).unwrap();
        assert_eq!(other.error_code, AiErrorCode::Other(27));
        let mut out = vec![];
        other.write_to(&mut out).unwrap();
        assert_eq!(out, bytes);
    }
}
