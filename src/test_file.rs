use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

enum ModifierState {
    Enable,
    Disable,
    Same,
}

#[derive(Default, Clone)]
pub struct SimpleTest {
    pub input: String,
    pub expected_output: String,
}

impl SimpleTest {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            expected_output: String::new(),
        }
    }
}

impl ModifierState {
    fn from_str(string: &str) -> Self {
        match string.to_lowercase().as_str() {
            "disable" => Self::Disable,
            "enable" => Self::Enable,
            _ => Self::Same,
        }
    }
}

pub struct TestFileIterator {
    reader: BufReader<File>,
    buffer: String,
    stack: i32,
    pub test_buffer: SimpleTest,
    arrow_amount: usize,
    pub states: HashMap<&'static str, bool>,
}

impl TestFileIterator {
    fn new(test_file_path: &Path) -> Result<Self, Box<dyn Error>> {
        let file = File::open(test_file_path)?;
        let reader = BufReader::new(file);

        Ok(Self {
            reader,
            buffer: String::with_capacity(1024), // Initial capacity for large buffer
            stack: 0,
            test_buffer: SimpleTest::new(),
            arrow_amount: 0,
            states: HashMap::from([
                ("standalone", false),
                ("trim", true),
                ("explicit-newline", false),
            ]),
        })
    }
}

impl Iterator for TestFileIterator {
    type Item = Result<SimpleTest, Box<dyn Error>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut ignore_next = false;

        for line in self.reader.by_ref().lines() {
            let line = match line {
                Ok(line) => line,
                Err(e) => return Some(Err(Box::new(e))),
            };

            if line.trim().starts_with('#') {
                let full_state = &line[(line.find('#').unwrap_or_default() + 1)..];
                let (modifier_state, state) = full_state
                    .split_once(':')
                    .map(|(mod_state, state)| (ModifierState::from_str(mod_state), state.trim()))
                    .unwrap_or((ModifierState::Enable, full_state));

                if let Some(state) = self.states.get_mut(state) {
                    match modifier_state {
                        ModifierState::Enable => *state = true,
                        ModifierState::Disable => *state = false,
                        _ => {}
                    }
                }
                continue;
            }

            let mut chars = line.chars().peekable();
            let mut line_count = 0u32;

            while let Some(chr) = chars.next() {
                line_count += 1;

                if ignore_next {
                    ignore_next = false;
                    continue;
                }

                match chr {
                    '{' => self.stack += 1,
                    '-' => {
                        if chars.peek() == Some(&'>') {
                            self.arrow_amount += 1;
                        } else if self.stack > 0 {
                            self.buffer.push('-');
                        }
                    }
                    '\\' => {
                        if let Some(&next) = chars.peek() {
                            if next == 'n' {
                                self.buffer.push('\n');
                                ignore_next = true;
                            } else if self.stack > 0 {
                                self.buffer.push('\\');
                            }
                        }
                    }
                    '}' => {
                        self.stack -= 1;
                        match self.stack.cmp(&0) {
                            std::cmp::Ordering::Less => {
                                return Some(Err(format!(
                                    "Bracket mismatch at line {}",
                                    line_count
                                )
                                .into()))
                            }

                            std::cmp::Ordering::Equal => {
                                let mut inner_buffer = self.buffer.clone();

                                if *self.states.get("trim").unwrap() {
                                    inner_buffer = inner_buffer
                                        .trim()
                                        .lines()
                                        .map(|line| line.trim())
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                }

                                if self.test_buffer.input.is_empty() {
                                    self.test_buffer.input = inner_buffer;
                                    self.buffer.clear();
                                    self.arrow_amount = 0;

                                    if *self.states.get("standalone").unwrap() {
                                        let test = std::mem::take(&mut self.test_buffer);
                                        return Some(Ok(test));
                                    }
                                } else if self.test_buffer.expected_output.is_empty()
                                    && self.arrow_amount == 1
                                {
                                    self.test_buffer.expected_output = inner_buffer;
                                    let test = std::mem::take(&mut self.test_buffer);
                                    self.buffer.clear();
                                    self.arrow_amount = 0;
                                    return Some(Ok(test));
                                } else if self.arrow_amount != 1 {
                                    return Some(Err(
                                        "Each input must be piped to output using '->'.".into(),
                                    ));
                                }
                            }
                            std::cmp::Ordering::Greater => {}
                        }
                    }
                    _ => {
                        if self.stack > 0 {
                            self.buffer.push(chr);
                        }
                    }
                }
            }

            if !self.states.get("explicit-newline").unwrap() {
                self.buffer.push('\n');
            }
        }

        None
    }
}

pub fn read_test_file(test_file_path: &Path) -> Result<TestFileIterator, Box<dyn Error>> {
    TestFileIterator::new(test_file_path)
}

pub struct MergedTestFileTterator {
    input_iterator: TestFileIterator,
    output_iterator: TestFileIterator,
}

impl MergedTestFileTterator {
    fn new(
        input_test_file_iterator: TestFileIterator,
        output_test_file_iterator: TestFileIterator,
    ) -> Result<MergedTestFileTterator, Box<dyn Error>> {
        Ok(MergedTestFileTterator {
            input_iterator: input_test_file_iterator,
            output_iterator: output_test_file_iterator,
        })
    }
}

impl Iterator for MergedTestFileTterator {
    type Item = Result<SimpleTest, Box<dyn Error>>;
    fn next(&mut self) -> Option<Self::Item> {
        let input_result = self.input_iterator.next()?;
        let output_result = self.output_iterator.next()?;

        let (input, expected_output) = match (input_result, output_result) {
            (Ok(input_test), Ok(output_test)) => (input_test.input, output_test.input),
            (Err(err), _) => {
                return Some(Err(err));
            }
            (_, Err(err)) => {
                return Some(Err(err));
            }
        };

        Some(Ok(SimpleTest {
            input,
            expected_output,
        }))
    }
}

pub fn merge_test_file(
    input_test_file_iterator: TestFileIterator,
    output_test_file_iterator: TestFileIterator,
) -> Result<MergedTestFileTterator, Box<dyn Error>> {
    MergedTestFileTterator::new(input_test_file_iterator, output_test_file_iterator)
}
