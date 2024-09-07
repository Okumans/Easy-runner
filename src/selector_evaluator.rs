use std::fmt;
use std::ops::RangeInclusive;

#[derive(Debug)]
pub enum RangeEvaluationError {
    InvalidRangeFormat(String),
    InvalidNumber(String),
}

impl fmt::Display for RangeEvaluationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RangeEvaluationError::InvalidRangeFormat(part) => {
                write!(f, "Invalid range format: {}", part)
            }
            RangeEvaluationError::InvalidNumber(part) => {
                write!(f, "Invalid number: {}", part)
            }
        }
    }
}

pub struct TestsRange {
    pub main_test: usize,
    pub sub_tests: Option<RangeInclusive<usize>>,
}

fn process_expression(expression: &str) -> Result<Vec<TestsRange>, RangeEvaluationError> {
    if expression.contains('-') {
        // Process main test case or sub-test range (e.g., "1.1-1.5" or "2-4")
        let parts: Vec<_> = expression.split('-').collect();

        if parts.len() != 2 {
            return Err(RangeEvaluationError::InvalidRangeFormat(
                "Range must have exactly one '-'".to_string(),
            ));
        }

        // Split both parts into main and sub-tests, if they contain '.'
        let left_parts: Vec<_> = parts[0].split('.').collect();
        let right_parts: Vec<_> = parts[1].split('.').collect();

        let main_start = left_parts[0]
            .parse::<usize>()
            .map_err(|_| RangeEvaluationError::InvalidNumber(left_parts[0].to_string()))?;
        let main_end = right_parts[0]
            .parse::<usize>()
            .map_err(|_| RangeEvaluationError::InvalidNumber(right_parts[0].to_string()))?;

        let sub_start = if left_parts.len() == 2 {
            left_parts[1]
                .parse::<usize>()
                .map_err(|_| RangeEvaluationError::InvalidNumber(left_parts[1].to_string()))?
        } else {
            1 // Default to sub-test 1 if not specified
        };

        let sub_end = if right_parts.len() == 2 {
            right_parts[1]
                .parse::<usize>()
                .map_err(|_| RangeEvaluationError::InvalidNumber(right_parts[1].to_string()))?
        } else {
            usize::MAX // Default to all sub-tests if not specified
        };

        let mut results = Vec::new();

        if main_start == main_end {
            // If the range is within the same main test case (e.g., "1.1-1.5")
            results.push(TestsRange {
                main_test: main_start,
                sub_tests: Some(sub_start..=sub_end),
            });
        } else {
            // Handle range across different main test cases (e.g., "3.2-4.2")
            results.push(TestsRange {
                main_test: main_start,
                sub_tests: Some(sub_start..=usize::MAX),
            }); // From sub_start to all remaining in main_start
            for main_test in (main_start + 1)..main_end {
                results.push(TestsRange {
                    main_test,
                    sub_tests: None,
                }); // Full sub-tests for main tests in between
            }
            results.push(TestsRange {
                main_test: main_end,
                sub_tests: Some(1..=sub_end),
            }); // From start in the last main test case to sub_end
        }

        Ok(results)
    } else if expression.contains('.') {
        // Handle single ref-test (e.g., "1.2")
        let parts: Vec<_> = expression.split('.').collect();

        if parts.len() != 2 {
            return Err(RangeEvaluationError::InvalidRangeFormat(
                "Ref-test format must have exactly one '.'".to_string(),
            ));
        }

        let main_test = parts[0]
            .parse::<usize>()
            .map_err(|_| RangeEvaluationError::InvalidNumber(parts[0].to_string()))?;
        let sub_test = parts[1]
            .parse::<usize>()
            .map_err(|_| RangeEvaluationError::InvalidNumber(parts[1].to_string()))?;

        Ok(vec![TestsRange {
            main_test,
            sub_tests: Some(sub_test..=sub_test),
        }])
    } else {
        // Handle single main test case (e.g., "2")
        let single_test = expression
            .parse::<usize>()
            .map_err(|_| RangeEvaluationError::InvalidNumber(expression.to_string()))?;
        Ok(vec![TestsRange {
            main_test: single_test,
            sub_tests: None,
        }]) // Include all sub-tests for the main test case
    }
}

pub fn evaluate(expressions: &str) -> Result<Vec<TestsRange>, RangeEvaluationError> {
    let mut result_expressions: Vec<TestsRange> = Vec::new();

    for expression in expressions.split(',') {
        match process_expression(expression.trim()) {
            Ok(mut result) => result_expressions.append(&mut result), // Append the flattened result
            Err(err) => return Err(err),
        }
    }

    Ok(result_expressions)
}
