use super::cache_file::Test;
use colored::Colorize;
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
    pub fn new() -> SimpleTest {
        SimpleTest {
            input: String::new(),
            expected_output: String::new(),
        }
    }

    pub fn to_test(self) -> Test {
        Test::StringTest {
            input: self.input,
            expected_output: self.expected_output,
        }
    }
}

impl ModifierState {
    fn from_str(string: &str) -> ModifierState {
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
    fn new(test_file_path: &Path) -> Result<TestFileIterator, Box<dyn Error>> {
        let file = File::open(test_file_path)?;
        let reader = BufReader::new(file);

        Ok(TestFileIterator {
            reader,
            buffer: String::new(),
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

        while let Some(line) = self.reader.by_ref().lines().next() {
            let line = match line {
                Ok(line) => line,
                Err(e) => return Some(Err(Box::new(e))),
            };

            // check if current line is a state modifier
            if line.trim().starts_with('#') {
                let full_state = &line[(line.find('#').unwrap_or_default() + 1)..];
                let (modifier_state, state) =
                    if let Some((modifier_state, state)) = full_state.split_once(':') {
                        (ModifierState::from_str(modifier_state), state.trim())
                    } else {
                        (ModifierState::Enable, full_state)
                    };

                if let Some(state) = self.states.get_mut(state) {
                    match modifier_state {
                        ModifierState::Enable => {
                            *state = true;
                        }
                        ModifierState::Disable => {
                            *state = false;
                        }
                        _ => {}
                    }
                }

                continue;
            }

            for (index, chr) in line.chars().enumerate() {
                if ignore_next {
                    ignore_next = false;
                    continue;
                }

                match chr {
                    '{' => {
                        self.stack += 1;
                    }

                    '-' => {
                        if let Some(next) = line.chars().nth(index + 1) {
                            if next == '>' {
                                self.arrow_amount += 1;
                            } else if self.stack >= 1 {
                                self.buffer.push('-');
                            }
                        }
                    }

                    '\\' => {
                        if !self.states.get("explicit-newline").unwrap() {
                            if self.stack >= 1 {
                                self.buffer.push('\\');
                            }
                            continue;
                        }

                        if let Some(next) = line.chars().nth(index + 1) {
                            if next == 'n' {
                                self.buffer.push('\n');
                                ignore_next = true;
                            } else if self.stack >= 1 {
                                self.buffer.push('\\');
                            }
                        }
                    }

                    '}' => {
                        self.stack -= 1;
                        match self.stack.cmp(&0) {
                            std::cmp::Ordering::Less => {
                                return Some(Err(format!("Bracket not matched at line {}. (Bracket closed without previously open)", index).into()));
                            }
                            std::cmp::Ordering::Equal => {
                                let mut inner_buffer = self.buffer.clone();

                                if *self.states.get("trim").unwrap() {
                                    inner_buffer = inner_buffer
                                        .trim()
                                        .split('\n')
                                        .map(|line| line.trim())
                                        .collect::<Vec<_>>()
                                        .join("\n")
                                }

                                if self.test_buffer.input.is_empty() {
                                    self.test_buffer.input = inner_buffer;
                                    self.arrow_amount = 0;
                                    self.buffer.clear();
                                    if *self.states.get("standalone").unwrap() {
                                        let test = std::mem::take(&mut self.test_buffer);
                                        self.test_buffer = SimpleTest::new();
                                        return Some(Ok(test));
                                    }
                                } else if self.test_buffer.expected_output.is_empty()
                                    && self.arrow_amount == 1
                                {
                                    self.test_buffer.expected_output = inner_buffer;
                                    let test = std::mem::take(&mut self.test_buffer);
                                    self.test_buffer = SimpleTest::new();
                                    self.buffer.clear();
                                    self.arrow_amount = 0;
                                    return Some(Ok(test));
                                } else if self.test_buffer.expected_output.is_empty()
                                    && self.arrow_amount != 1
                                {
                                    return Some(Err("With the state \"standalone\" disabled, every input must once be piped to output using \"->\".".into()));
                                }
                            }
                            _ => {}
                        }
                    }

                    _ => {
                        if self.stack >= 1 {
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
            _ => return Some(Err("Ref test cannot be used in test file iterator.".into())),
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

// pub fn read_test_file(test_file_path: &Path) -> Result<Vec<Test>, Box<dyn Error>> {
//     let mut tests: Vec<Test> = Vec::new();
//     let mut states: HashMap<&'static str, bool> = HashMap::from([
//         ("standalone", false),
//         ("trim", true),
//         ("explicit-newline", false),
//     ]);
//
//     let file = File::open(test_file_path)?;
//     let reader = BufReader::new(file);
//
//     let mut stack: i32 = 0;
//     let mut buffer = String::new();
//     let mut test_buffer = SimpleTest::new();
//     let mut arrow_amount: usize = 0;
//
//     for line in reader.lines() {
//         let Ok(line) = line else {
//             continue;
//         };
//
//         // check if current line is a state modifier
//         if line.trim().starts_with('#') {
//             let full_state = &line[(line.find('#').unwrap_or_default() + 1)..];
//             let (modifier_state, state) =
//                 if let Some((modifier_state, state)) = full_state.split_once(':') {
//                     (ModifierState::from_str(modifier_state), state.trim())
//                 } else {
//                     (ModifierState::Enable, full_state)
//                 };
//
//             if let Some(state) = states.get_mut(state) {
//                 match modifier_state {
//                     ModifierState::Enable => {
//                         *state = true;
//                     }
//                     ModifierState::Disable => {
//                         *state = false;
//                     }
//                     _ => {}
//                 }
//             }
//
//             continue;
//         }
//
//         let mut ignore_next = false;
//         for (index, chr) in line.chars().enumerate() {
//             if ignore_next {
//                 ignore_next = false;
//                 continue;
//             }
//
//             match chr {
//                 '{' => {
//                     stack += 1;
//                 }
//
//                 '-' => {
//                     if let Some(next) = line.chars().nth(index + 1) {
//                         if next == '>' {
//                             arrow_amount += 1;
//                         } else if stack >= 1 {
//                             buffer.push('-');
//                         }
//                     }
//                 }
//
//                 '\\' => {
//                     if !states.get("explicit-newline").unwrap() {
//                         if stack >= 1 {
//                             buffer.push('\\');
//                         }
//                         continue;
//                     }
//
//                     if let Some(next) = line.chars().nth(index + 1) {
//                         if next == 'n' {
//                             buffer.push('\n');
//                             ignore_next = true;
//                         } else if stack >= 1 {
//                             buffer.push('\\');
//                         }
//                     }
//                 }
//
//                 '}' => {
//                     stack -= 1;
//                     match stack.cmp(&0) {
//                         std::cmp::Ordering::Less => {
//                             return Err(format!("Bracket not mathed at line {}. (Bracket closed without previously open)", index).into());
//                         }
//                         std::cmp::Ordering::Equal => {
//                             let mut inner_buffer = buffer.clone();
//
//                             if *states.get("trim").unwrap() {
//                                 inner_buffer =
//                                     inner_buffer.split('\n').map(|line| line.trim()).join("\n");
//                             }
//
//                             if test_buffer.input.is_empty() {
//                                 test_buffer.input = inner_buffer;
//                                 if *states.get("standalone").unwrap() {
//                                     tests.push(test_buffer.to_test());
//                                     test_buffer = SimpleTest::new();
//                                 }
//                                 buffer.clear();
//                                 arrow_amount = 0;
//                             } else if test_buffer.expected_output.is_empty() && arrow_amount == 1 {
//                                 test_buffer.expected_output = inner_buffer;
//                                 tests.push(test_buffer.to_test());
//                                 test_buffer = SimpleTest::new();
//                                 buffer.clear();
//                                 arrow_amount = 0;
//                             } else if test_buffer.expected_output.is_empty() && arrow_amount != 1 {
//                                 return Err("With the state \"standalone\" enable every input must once be piped to output using \"->\".".into());
//                             }
//                         }
//                         _ => {}
//                     }
//                 }
//
//                 _ => {
//                     if stack >= 1 {
//                         buffer.push(chr);
//                     }
//                 }
//             }
//         }
//
//         if !states.get("explicit-newline").unwrap() {
//             buffer.push('\n');
//         }
//     }
//
//     Ok(tests)
// }
