//! CPA-005-style fixed-width AFT file codec. Authentic in shape (header / detail
//! per entry / trailer with totals), round-trippable — not byte-exact to the
//! 1464-byte CPA-005 logical-record spec.

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

#[derive(Debug, Clone)]
pub struct Header {
    pub originator_id: String,
    pub created: String,
    pub file_seq: u32,
}

#[derive(Debug, Clone)]
pub struct Detail {
    pub txn_code: char, // 'C' | 'D'
    pub amount: Decimal,
    pub institution: String,
    pub transit: String,
    pub account: String,
    pub payee_name: String,
    pub originator_short: String,
    pub due_date: String,
    pub return_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Trailer {
    pub entry_count: u32,
    pub total_credits: Decimal,
    pub total_debits: Decimal,
}

#[derive(Debug, thiserror::Error)]
pub enum CpaError {
    #[error("malformed CPA-005 file: {0}")]
    Malformed(String),
}

fn field(s: &str, width: usize) -> String {
    let mut t: String = s.chars().take(width).collect();
    while t.len() < width {
        t.push(' ');
    }
    t
}

fn cents(a: Decimal) -> String {
    format!("{:010}", (a * Decimal::from(100)).round().to_i64().unwrap_or(0))
}

fn parse_cents(s: &str) -> Decimal {
    Decimal::new(s.trim().parse::<i64>().unwrap_or(0), 2)
}

pub fn encode(h: &Header, details: &[Detail], t: &Trailer) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "H{}{}{:06}\n",
        field(&h.originator_id, 10),
        field(&h.created, 7),
        h.file_seq
    ));
    for d in details {
        out.push_str(&format!(
            "{}{}{}{}{}{}{}{}{}\n",
            d.txn_code,                                          // 1  (C|D)
            cents(d.amount),                                     // 10
            field(&d.institution, 3),                            // 3
            field(&d.transit, 5),                                // 5
            field(&d.account, 12),                               // 12
            field(&d.payee_name, 30),                            // 30
            field(&d.originator_short, 4),                       // 4
            field(&d.due_date, 7),                               // 7
            field(d.return_reason.as_deref().unwrap_or(""), 4),  // 4
        ));
    }
    out.push_str(&format!(
        "T{:06}{}{}\n",
        t.entry_count,
        cents(t.total_credits),
        cents(t.total_debits)
    ));
    out
}

pub fn decode(s: &str) -> Result<(Header, Vec<Detail>, Trailer), CpaError> {
    let mut header = None;
    let mut details = Vec::new();
    let mut trailer = None;
    for line in s.lines() {
        if line.is_empty() {
            continue;
        }
        match line.chars().next() {
            Some('H') => {
                header = Some(Header {
                    originator_id: line[1..11].trim().to_string(),
                    created: line[11..18].trim().to_string(),
                    file_seq: line[18..24].trim().parse().unwrap_or(0),
                })
            }
            Some(c @ ('C' | 'D')) => details.push(Detail {
                txn_code: c,
                amount: parse_cents(&line[1..11]),
                institution: line[11..14].trim().to_string(),
                transit: line[14..19].trim().to_string(),
                account: line[19..31].trim().to_string(),
                payee_name: line[31..61].trim().to_string(),
                originator_short: line[61..65].trim().to_string(),
                due_date: line[65..72].trim().to_string(),
                return_reason: {
                    let r = line.get(72..76).unwrap_or("").trim();
                    if r.is_empty() {
                        None
                    } else {
                        Some(r.to_string())
                    }
                },
            }),
            Some('T') => {
                trailer = Some(Trailer {
                    entry_count: line[1..7].trim().parse().unwrap_or(0),
                    total_credits: parse_cents(&line[7..17]),
                    total_debits: parse_cents(&line[17..27]),
                })
            }
            _ => return Err(CpaError::Malformed(format!("unknown record: {line}"))),
        }
    }
    Ok((
        header.ok_or_else(|| CpaError::Malformed("missing header".into()))?,
        details,
        trailer.ok_or_else(|| CpaError::Malformed("missing trailer".into()))?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (Header, Vec<Detail>, Trailer) {
        let h = Header {
            originator_id: "0000000900".into(),
            created: "2026185".into(),
            file_seq: 1,
        };
        let d = vec![
            Detail {
                txn_code: 'C',
                amount: Decimal::new(12345, 2),
                institution: "003".into(),
                transit: "00001".into(),
                account: "000000000001".into(),
                payee_name: "ALICE EXAMPLE".into(),
                originator_short: "NANO".into(),
                due_date: "2026186".into(),
                return_reason: None,
            },
            Detail {
                txn_code: 'D',
                amount: Decimal::new(5000, 2),
                institution: "004".into(),
                transit: "00002".into(),
                account: "000000000002".into(),
                payee_name: "BOB PAYER".into(),
                originator_short: "NANO".into(),
                due_date: "2026186".into(),
                return_reason: None,
            },
        ];
        let t = Trailer {
            entry_count: 2,
            total_credits: Decimal::new(12345, 2),
            total_debits: Decimal::new(5000, 2),
        };
        (h, d, t)
    }

    #[test]
    fn round_trips() {
        let (h, d, t) = sample();
        let encoded = encode(&h, &d, &t);
        let (h2, d2, t2) = decode(&encoded).expect("decode");
        assert_eq!(h.originator_id, h2.originator_id);
        assert_eq!(h.created, h2.created);
        assert_eq!(d.len(), d2.len());
        assert_eq!(d[0].amount, d2[0].amount);
        assert_eq!(d[0].institution, d2[0].institution);
        assert_eq!(d[0].account, d2[0].account);
        assert_eq!(d[0].payee_name, d2[0].payee_name);
        assert_eq!(d[1].txn_code, d2[1].txn_code);
        assert_eq!(d[1].amount, d2[1].amount);
        assert_eq!(t.entry_count, t2.entry_count);
        assert_eq!(t.total_credits, t2.total_credits);
        assert_eq!(t.total_debits, t2.total_debits);
    }

    #[test]
    fn trailer_totals_match_details() {
        let (_h, d, t) = sample();
        let credits: Decimal = d.iter().filter(|x| x.txn_code == 'C').map(|x| x.amount).sum();
        let debits: Decimal = d.iter().filter(|x| x.txn_code == 'D').map(|x| x.amount).sum();
        assert_eq!(credits, t.total_credits);
        assert_eq!(debits, t.total_debits);
    }

    #[test]
    fn return_reason_round_trips() {
        let (h, mut d, t) = sample();
        d[1].return_reason = Some("NSF".into());
        let (_h, d2, _t) = decode(&encode(&h, &d, &t)).expect("decode");
        assert_eq!(d2[1].return_reason.as_deref(), Some("NSF"));
    }
}
