use crate::data::value::{format_leaf, style_leaf};
use crate::format::RenderView;
use crate::prelude::*;
use derive_new::new;
use nu_errors::ShellError;
use nu_protocol::{UntaggedValue, Value};
use textwrap::fill;

use prettytable::format::{FormatBuilder, LinePosition, LineSeparator};
use prettytable::{color, Attr, Cell, Row, Table};

#[derive(Debug, new)]
pub struct TableView {
    // List of header cell values:
    headers: Vec<String>,

    // List of rows of cells, each containing value and prettytable style-string:
    entries: Vec<Vec<(String, &'static str)>>,
}

enum TableMode {
    Light,
    Normal,
}

impl TableView {
    fn merge_descriptors(values: &[Value]) -> Vec<String> {
        let mut ret: Vec<String> = vec![];
        let value_column = "<value>".to_string();
        for value in values {
            let descs = value.data_descriptors();

            if descs.is_empty() {
                if !ret.contains(&value_column) {
                    ret.push("<value>".to_string());
                }
            } else {
                for desc in value.data_descriptors() {
                    if !ret.contains(&desc) {
                        ret.push(desc);
                    }
                }
            }
        }
        ret
    }

    pub fn from_list(values: &[Value], starting_idx: usize) -> Option<TableView> {
        if values.is_empty() {
            return None;
        }

        let mut headers = TableView::merge_descriptors(values);

        if headers.is_empty() {
            headers.push("<value>".to_string());
        }

        let mut entries = vec![];

        for (idx, value) in values.iter().enumerate() {
            let mut row: Vec<(String, &'static str)> = headers
                .iter()
                .map(|d| {
                    if d == "<value>" {
                        match value {
                            Value {
                                value: UntaggedValue::Row(..),
                                ..
                            } => (
                                format_leaf(&UntaggedValue::nothing()).plain_string(100_000),
                                style_leaf(&UntaggedValue::nothing()),
                            ),
                            _ => (format_leaf(value).plain_string(100_000), style_leaf(value)),
                        }
                    } else {
                        match value {
                            Value {
                                value: UntaggedValue::Row(..),
                                ..
                            } => {
                                let data = value.get_data(d);
                                (
                                    format_leaf(data.borrow()).plain_string(100_000),
                                    style_leaf(data.borrow()),
                                )
                            }
                            _ => (
                                format_leaf(&UntaggedValue::nothing()).plain_string(100_000),
                                style_leaf(&UntaggedValue::nothing()),
                            ),
                        }
                    }
                })
                .collect();

            if values.len() > 1 {
                // Indices are black, bold, right-aligned:
                row.insert(0, ((starting_idx + idx).to_string(), "Fdbr"));
            }

            entries.push(row);
        }

        let mut max_per_column = vec![];

        if values.len() > 1 {
            headers.insert(0, "#".to_owned());
        }

        for i in 0..headers.len() {
            let mut current_col_max = 0;
            let iter = entries.iter().take(values.len());

            for entry in iter {
                let value_length = entry[i].0.chars().count();
                if value_length > current_col_max {
                    current_col_max = value_length;
                }
            }

            max_per_column.push(std::cmp::max(current_col_max, headers[i].chars().count()));
        }

        // Different platforms want different amounts of buffer, not sure why
        let termwidth = std::cmp::max(textwrap::termwidth(), 20);

        // Make sure we have enough space for the columns we have
        let max_num_of_columns = termwidth / 10;

        // If we have too many columns, truncate the table
        if max_num_of_columns < headers.len() {
            headers.truncate(max_num_of_columns);

            for entry in &mut entries {
                entry.truncate(max_num_of_columns);
            }

            headers.push("...".to_owned());
            for entry in &mut entries {
                entry.push(("...".to_owned(), "c")); // ellipsis is centred
            }
        }

        // Measure how big our columns need to be (accounting for separators also)
        let max_naive_column_width = (termwidth - 3 * (headers.len() - 1)) / headers.len();

        // Measure how much space we have once we subtract off the columns who are small enough
        let mut num_overages = 0;
        let mut underage_sum = 0;
        let mut overage_separator_sum = 0;
        let iter = max_per_column.iter().enumerate().take(headers.len());
        for (i, &column_max) in iter {
            if column_max > max_naive_column_width {
                num_overages += 1;
                if i != (headers.len() - 1) {
                    overage_separator_sum += 3;
                }
                if i == 0 {
                    overage_separator_sum += 1;
                }
            } else {
                underage_sum += column_max;
                // if column isn't last, add 3 for its separator
                if i != (headers.len() - 1) {
                    underage_sum += 3;
                }
                if i == 0 {
                    underage_sum += 1;
                }
            }
        }

        // This gives us the max column width
        let max_column_width = if num_overages > 0 {
            (termwidth - 1 - underage_sum - overage_separator_sum) / num_overages
        } else {
            99999
        };

        // This width isn't quite right, as we're rounding off some of our space
        num_overages = 0;
        overage_separator_sum = 0;
        let iter = max_per_column.iter().enumerate().take(headers.len());
        for (i, &column_max) in iter {
            if column_max > max_naive_column_width {
                if column_max <= max_column_width {
                    underage_sum += column_max;
                    // if column isn't last, add 3 for its separator
                    if i != (headers.len() - 1) {
                        underage_sum += 3;
                    }
                    if i == 0 {
                        underage_sum += 1;
                    }
                } else {
                    // Column is still too large, so let's count it
                    num_overages += 1;
                    if i != (headers.len() - 1) {
                        overage_separator_sum += 3;
                    }
                    if i == 0 {
                        overage_separator_sum += 1;
                    }
                }
            }
        }
        // This should give us the final max column width
        let max_column_width = if num_overages > 0 {
            (termwidth - 1 - underage_sum - overage_separator_sum) / num_overages
        } else {
            99999
        };

        // Wrap cells as needed
        for head in 0..headers.len() {
            if max_per_column[head] > max_naive_column_width {
                headers[head] = fill(&headers[head], max_column_width);

                for entry in &mut entries {
                    entry[head].0 = fill(&entry[head].0, max_column_width);
                }
            }
        }

        Some(TableView { headers, entries })
    }
}

impl RenderView for TableView {
    fn render_view(&self, host: &mut dyn Host) -> Result<(), ShellError> {
        if self.entries.is_empty() {
            return Ok(());
        }

        let mut table = Table::new();

        let table_mode = crate::data::config::config(Tag::unknown())?
            .get("table_mode")
            .map(|s| match s.as_string().unwrap().as_ref() {
                "light" => TableMode::Light,
                _ => TableMode::Normal,
            })
            .unwrap_or(TableMode::Normal);

        match table_mode {
            TableMode::Light => {
                table.set_format(
                    FormatBuilder::new()
                        .separator(LinePosition::Title, LineSeparator::new('─', '─', ' ', ' '))
                        .padding(1, 1)
                        .build(),
                );
            }
            _ => {
                table.set_format(
                    FormatBuilder::new()
                        .column_separator('│')
                        .separator(LinePosition::Top, LineSeparator::new('━', '┯', ' ', ' '))
                        .separator(LinePosition::Title, LineSeparator::new('─', '┼', ' ', ' '))
                        .separator(LinePosition::Bottom, LineSeparator::new('━', '┷', ' ', ' '))
                        .padding(1, 1)
                        .build(),
                );
            }
        }

        let header: Vec<Cell> = self
            .headers
            .iter()
            .map(|h| {
                Cell::new(h)
                    .with_style(Attr::ForegroundColor(color::GREEN))
                    .with_style(Attr::Bold)
            })
            .collect();

        table.set_titles(Row::new(header));

        for row in &self.entries {
            table.add_row(Row::new(
                row.iter()
                    .map(|(v, s)| Cell::new(v).style_spec(s))
                    .collect(),
            ));
        }

        table.print_term(&mut *host.out_terminal()).unwrap();

        Ok(())
    }
}
