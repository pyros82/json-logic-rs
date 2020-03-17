//! Operators
//!

use phf::phf_map;
use serde_json::{Map, Value};
use std::convert::TryFrom;
use std::fmt;

use crate::error::Error;
use crate::js_op::abstract_eq;
use crate::value::{Evaluated, Parsed};

pub struct Operator {
    symbol: &'static str,
    operator: OperatorFn<'static>,
}
impl Operator {
    pub fn execute<'a>(&self, items: &Vec<Evaluated>) -> Result<Evaluated<'a>, Error> {
        (self.operator)(items)
    }
}
impl fmt::Debug for Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Operator")
            .field("symbol", &self.symbol)
            .field("operator", &"<operator fn>")
            .finish()
    }
}

type OperatorFn<'a> = fn(&Vec<Evaluated>) -> Result<Evaluated<'a>, Error>;

/// Operator for JS-style abstract equality
pub fn op_abstract_eq<'a>(items: &Vec<Evaluated>) -> Result<Evaluated<'a>, Error> {
    let to_res = |first: &Value, second: &Value| -> Result<Evaluated<'a>, Error> {
        Ok(Evaluated::New(Value::Bool(abstract_eq(&first, &second))))
    };

    match items[..] {
        [Evaluated::Raw(first), Evaluated::Raw(second)] => to_res(first, second),
        [Evaluated::New(ref first), Evaluated::Raw(second)] => to_res(first, second),
        [Evaluated::Raw(first), Evaluated::New(ref second)] => to_res(first, second),
        [Evaluated::New(ref first), Evaluated::New(ref second)] => to_res(first, second),
        _ => Err(Error::WrongArgumentCount {
            expected: 2,
            actual: items.len(),
        }),
    }
}

pub static OPERATOR_MAP: phf::Map<&'static str, Operator> = phf_map! {
    "==" => Operator {symbol: "==", operator: op_abstract_eq}
};

#[derive(Debug)]
pub struct Operation<'a> {
    operator: &'a Operator,
    arguments: Vec<Parsed<'a>>,
}
impl<'a> Operation<'a> {
    pub fn from_value(value: &'a Value) -> Result<Option<Self>, Error> {
        match value {
            // Operations are Objects
            Value::Object(obj) => {
                // Operations have just one key/value pair.
                if obj.len() != 1 {
                    return Ok(None);
                }

                let key = obj.keys().next().ok_or(Error::UnexpectedError(format!(
                    "could not get first key from len(1) object: {:?}",
                    obj
                )))?;
                let val = obj.get(key).ok_or(Error::UnexpectedError(format!(
                    "could not get value for key '{}' from len(1) object: {:?}",
                    key, obj
                )))?;

                // Operators have known keys
                if let Some(operator) = OPERATOR_MAP.get(key.as_str()) {
                    match val {
                        // Operator values are arrays
                        Value::Array(arguments) => Ok(Some(Operation {
                            operator,
                            arguments: Parsed::from_values(arguments)?,
                        })),
                        _ => Err(Error::InvalidOperation {
                            key: key.into(),
                            reason: "Values for operator keys must be arrays".into(),
                        }),
                    }
                } else {
                    Ok(None)
                }
            }
            // We're definitely not an operation
            _ => Ok(None),
        }
    }
    /// Evaluate the operation after recursively evaluating any nested operations
    pub fn evaluate(&self, data: &Value) -> Result<Evaluated, Error> {
        let arguments = self
            .arguments
            .iter()
            .map(|value| value.evaluate(data))
            .collect::<Result<Vec<Evaluated>, Error>>()?;
        self.operator.execute(&arguments)
    }
}

impl TryFrom<Operation<'_>> for Value {
    type Error = Error;

    fn try_from(op: Operation) -> Result<Self, Self::Error> {
        let mut rv = Map::with_capacity(1);
        let values = op
            .arguments
            .into_iter()
            .map(Value::try_from)
            .collect::<Result<Vec<Self>, Self::Error>>()?;
        rv.insert(op.operator.symbol.into(), Value::Array(values));
        Ok(Value::Object(rv))
    }
}

/// Return whether a value is "truthy" by the JSONLogic spec
///
/// The spec (http://jsonlogic.com/truthy) defines truthy values that
/// diverge slightly from raw JavaScript. This ensures a matching
/// interpretation.
///
/// In general, the spec specifies that values are truthy or falsey
/// depending on their containing something, e.g. non-zero integers,
/// non-zero length strings, and non-zero length arrays are truthy.
/// This does not apply to objects, which are always truthy.
pub fn truthy(val: &Value) -> bool {
    match val {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v
            .as_f64()
            .map(|v_num| if v_num == 0.0 { false } else { true })
            .unwrap_or(false),
        Value::String(v) => {
            if v == "" {
                false
            } else {
                true
            }
        }
        Value::Array(v) => {
            if v.len() == 0 {
                false
            } else {
                true
            }
        }
        Value::Object(_) => true,
    }
}

#[cfg(test)]
mod test_truthy {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_truthy() {
        let trues = [
            json!(true),
            json!([1]),
            json!([1, 2]),
            json!({}),
            json!({"a": 1}),
            json!(1),
            json!(-1),
            json!("foo"),
        ];

        let falses = [json!(false), json!([]), json!(""), json!(0), json!(null)];

        trues.iter().for_each(|v| assert!(truthy(&v)));
        falses.iter().for_each(|v| assert!(!truthy(&v)));
    }
}