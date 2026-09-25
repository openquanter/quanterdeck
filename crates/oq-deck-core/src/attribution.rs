//! The gap between a live result and the model's, decomposed.
//!
//! # What this can and cannot know
//!
//! `oq_parity::attribution::attribute` does the decomposition. It is not
//! re-implemented here and must not be: the rule that a cause nobody
//! measured is `Unavailable` rather than `Cash(0)` is the whole value of
//! the report, and a second implementation is a second chance to get it
//! wrong.
//!
//! What this module supplies is the evidence, assembled from two run
//! files. That is a **weaker** source than a live shadow, and the
//! difference is not cosmetic:
//!
//! * A shadow sees the price prevailing when each fill happened. A run
//!   file does not record it. `Matched::reference_price` is therefore
//!   `None` here, which upstream defines as making **slippage and
//!   latency unavailable for the whole report** — the two cannot be
//!   separated without it, and a number silently containing both is
//!   worse than no number.
//! * Funding and fees come from the venue's own statements. The console
//!   has no venue connection, so they arrive from the operator or not at
//!   all.
//!
//! The consequence is that an attribution built this way will usually
//! decline to produce a residual. That is the correct answer, and the
//! report says which inputs would change it. A console that filled the
//! holes with zeroes would show a gap fully explained by causes nobody
//! looked at, which is the exact failure `FR-ATTRIB-6` exists to stop.
//!
//! # Where the pairing comes from
//!
//! Not from here either. `oq_parity::diff::compare` aligns the two fill
//! streams with a resync window, and this module reads its output:
//! a `Mismatch` is one fill both sides made, a `Missing` is one only the
//! model made, an `Extra` is one only the venue made, and every index
//! not named in a difference is an exact agreement. Inventing a second
//! pairing would give the console a private opinion about which fills
//! correspond, and it would disagree with the parity report shown two
//! screens away.

use std::path::Path;

use oq_parity::attribution::{Attributed, Component, Evidence, Matched, Unmatched, attribute};
use oq_parity::diff::{Difference, compare};
use oq_parity::record::Fill;
use oq_types::{CASH_SCALE, Cash, Instrument};
use serde::{Deserialize, Serialize};

use crate::runs;

/// One cause, as the interface shows it.
#[derive(Debug, Clone, Serialize)]
pub struct ComponentView {
    pub name: &'static str,
    /// Whether this is a difference between two observed quantities.
    /// `false` means the number required a decision about what something
    /// was worth. Both belong in the report; conflating them does not.
    pub observed: bool,
    /// The amount, when it could be computed.
    pub amount: Option<f64>,
    /// Why it could not be, when it could not. Shown verbatim.
    pub unavailable: Option<String>,
}

/// The report.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub live_run: String,
    pub model_run: String,
    pub live_pnl: f64,
    pub model_pnl: f64,
    /// `live - model`, computed from those two and from nothing else.
    pub gap: f64,
    pub components: Vec<ComponentView>,
    /// What will not decompose.
    ///
    /// `None` when any component is unavailable. Serialised as JSON null
    /// so the interface cannot mistake it for zero — a residual computed
    /// against an incomplete decomposition is a number that looks like
    /// the product and is not one.
    pub residual: Option<f64>,
    /// The residual as a share of the live result, when there is one.
    pub residual_share: Option<f64>,
    /// How the evidence was assembled. `run-files` is the weak source
    /// described in this module's documentation; a future `shadow` would
    /// be the strong one.
    pub method: &'static str,
    /// What would have to exist for the unavailable causes to become
    /// available. Shown to the operator, because "unavailable" without
    /// "and here is what would fix it" is a dead end.
    pub missing_inputs: Vec<String>,
    pub matched_fills: usize,
    pub unmatched_fills: usize,
}

/// Funding or fees, as the venue charged and as the model computed.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct VenueVsModel {
    pub venue: f64,
    pub model: f64,
}

impl VenueVsModel {
    fn to_cash(self) -> (Cash, Cash) {
        (to_cash(self.venue), to_cash(self.model))
    }
}

/// What the caller supplies beside the two runs.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Inputs {
    /// Decimal places in a price, as the instrument quotes it.
    pub price_scale: u8,
    /// Decimal places in a quantity.
    pub qty_scale: u8,
    /// Funding the venue charged against funding the model computed.
    /// Absent means the funding component is unavailable, not zero.
    pub funding: Option<VenueVsModel>,
    /// Fees the venue charged against fees the model computed.
    pub fees: Option<VenueVsModel>,
}

fn to_cash(units: f64) -> Cash {
    // Cash counts in hundred-millionths of a quote-currency unit; a run
    // file writes P&L as a decimal. Rounding here is the only place the
    // two representations meet, so it happens once and to nearest.
    #[allow(clippy::cast_possible_truncation)]
    Cash((units * CASH_SCALE as f64).round() as i64)
}

#[allow(clippy::cast_precision_loss)]
fn from_cash(cash: Cash) -> f64 {
    cash.0 as f64 / CASH_SCALE as f64
}

fn matched_from(model: &Fill, venue: &Fill) -> Option<Matched> {
    // Different side, size or instrument is not the same event, whatever
    // position it occupies. Those become two unmatched fills instead —
    // two instruments' fills of one side and size were paired, and their
    // price difference charged to slippage.
    (model.symbol == venue.symbol && model.side == venue.side && model.qty == venue.qty).then_some(
        Matched {
            side: model.side,
            qty: model.qty,
            model_price: model.price,
            venue_price: venue.price,
            // A run file records no prevailing price. Upstream reads this
            // `None` as "slippage and latency cannot be separated", and that
            // is exactly the situation.
            reference_price: None,
        },
    )
}

fn unmatched_from(fill: &Fill, at_venue: bool) -> Unmatched {
    Unmatched {
        side: fill.side,
        qty: fill.qty,
        price: fill.price,
        reference_price: None,
        at_venue,
    }
}

/// Assemble evidence from an alignment the framework produced.
///
/// Only the model-side fill list is needed: an index the alignment did
/// not name agreed exactly, and for those the venue price *is* the model
/// price, so reading it from the other stream would add a second way to
/// get the same value.
fn evidence_from(model_fills: &[Fill], differences: &[Difference]) -> Evidence {
    let mut matched = Vec::new();
    let mut unmatched = Vec::new();
    // Every model-side index the alignment accounted for. Whatever is
    // left agreed exactly, which is how the exact matches are recovered
    // without walking the streams a second time.
    let mut accounted = vec![false; model_fills.len()];

    for difference in differences {
        match difference {
            Difference::Mismatch {
                index,
                baseline,
                candidate,
                ..
            } => {
                if let Some(pair) = matched_from(baseline, candidate) {
                    matched.push(pair);
                } else {
                    unmatched.push(unmatched_from(baseline, false));
                    unmatched.push(unmatched_from(candidate, true));
                }
                if let Some(slot) = accounted.get_mut(*index) {
                    *slot = true;
                }
            }
            Difference::Missing { index, fill } => {
                unmatched.push(unmatched_from(fill, false));
                if let Some(slot) = accounted.get_mut(*index) {
                    *slot = true;
                }
            }
            Difference::Extra { fill, .. } => {
                unmatched.push(unmatched_from(fill, true));
            }
        }
    }

    for (index, seen) in accounted.iter().enumerate() {
        if !seen {
            let fill = &model_fills[index];
            matched.push(Matched {
                side: fill.side,
                qty: fill.qty,
                model_price: fill.price,
                venue_price: fill.price,
                reference_price: None,
            });
        }
    }

    Evidence {
        matched,
        unmatched,
        funding: None,
        // Replaced below when the operator supplies both figures.
        funding_unavailable: Some(
            "run files carry no funding; enter what the venue charged and what the model \
             computed to measure it"
                .to_string(),
        ),
        fees: None,
    }
}

fn view(component: Component, attributed: &Attributed) -> ComponentView {
    ComponentView {
        name: component.label(),
        observed: component.is_observed(),
        amount: attributed.amount().map(from_cash),
        unavailable: match attributed {
            Attributed::Explained(_) => None,
            Attributed::Unavailable(why) => Some(why.clone()),
        },
    }
}

/// Decompose the gap between two runs.
///
/// The model run is the baseline and the live run the candidate, so the
/// framework's `Missing` is a fill the model made alone and its `Extra`
/// is one the venue made alone.
///
/// # Errors
/// Either run is missing or will not read.
pub fn from_runs(
    dir: &Path,
    live_id: &str,
    model_id: &str,
    inputs: Inputs,
) -> Result<Report, String> {
    let live = runs::read(dir, live_id)?;
    let model = runs::read(dir, model_id)?;

    let report = compare(&model.manifest, &model.output, &live.manifest, &live.output);

    let mut evidence = evidence_from(&model.output.fills, &report.differences);
    evidence.funding = inputs.funding.map(VenueVsModel::to_cash);
    if evidence.funding.is_some() {
        evidence.funding_unavailable = None;
    }
    evidence.fees = inputs.fees.map(VenueVsModel::to_cash);

    let matched_fills = evidence.matched.len();
    let unmatched_fills = evidence.unmatched.len();

    let live_pnl = to_cash(live.output.pnl);
    let model_pnl = to_cash(model.output.pnl);
    let instrument = Instrument::linear(inputs.price_scale, inputs.qty_scale);

    // The manifest travels with the report so a third party can
    // reproduce it. It is the live run's, because that is the thing
    // being explained.
    let attribution = attribute(
        live.manifest.clone(),
        &instrument,
        live_pnl,
        model_pnl,
        &evidence,
    );

    let mut missing_inputs = Vec::new();
    if attribution.components.iter().any(|(c, a)| {
        matches!(a, Attributed::Unavailable(_))
            && !matches!(c, Component::Funding | Component::FeeTier)
    }) {
        missing_inputs.push(
            "成交发生时的市场价（reference price）——run 文件没有这一列，\
             缺了它 slippage 与 latency 无法分开。要补上它，需要一次 shadow \
             运行的证据，或让 run 格式记录它。"
                .to_owned(),
        );
    }
    if inputs.funding.is_none() {
        missing_inputs.push("交易所的资金费结算，与模型计算的资金费。".to_owned());
    }
    if inputs.fees.is_none() {
        missing_inputs.push("交易所的手续费结算，与模型计算的手续费。".to_owned());
    }

    Ok(Report {
        live_run: live_id.to_owned(),
        model_run: model_id.to_owned(),
        live_pnl: live.output.pnl,
        model_pnl: model.output.pnl,
        gap: from_cash(attribution.gap),
        components: attribution
            .components
            .iter()
            .map(|(component, attributed)| view(*component, attributed))
            .collect(),
        residual: attribution.residual.map(from_cash),
        residual_share: attribution.residual_share(),
        method: "run-files",
        missing_inputs,
        matched_fills,
        unmatched_fills,
    })
}

#[cfg(test)]
mod pairing {
    use super::matched_from;
    use oq_parity::Fill;
    use oq_types::Side;

    /// Two instruments' fills of one side and size are not one event.
    #[test]
    fn fills_of_different_instruments_are_not_paired() {
        let btc = Fill::new(1, "BTCUSDT", Side::Buy, 100, 5);
        let eth = Fill::new(1, "ETHUSDT", Side::Buy, 90, 5);
        assert!(matched_from(&btc, &eth).is_none());
        assert!(matched_from(&btc, &Fill::new(1, "BTCUSDT", Side::Buy, 101, 5)).is_some());
    }
}
