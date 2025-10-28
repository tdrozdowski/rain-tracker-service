/// List all gauge IDs from a water year Excel file
use calamine::{open_workbook, Data, Reader, Xlsx};
use std::collections::HashSet;
use std::env;
use std::fs::File;
use std::io::BufReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let path = if args.len() > 1 {
        &args[1]
    } else {
        "/tmp/pcp_WY_2023.xlsx"
    };

    println!("Reading gauge IDs from: {path}");

    let mut workbook: Xlsx<BufReader<File>> = open_workbook(path)?;

    // Read OCT sheet to get gauge IDs from Row 3
    let range = workbook.worksheet_range("OCT")?;

    let rows: Vec<_> = range.rows().collect();
    if rows.len() < 3 {
        return Err("Not enough rows in sheet".into());
    }

    // Row 3 (index 2): Gauge IDs in columns B onward
    let gauge_row = &rows[2];
    let mut gauge_ids = HashSet::new();

    for cell in gauge_row.iter().skip(1) {
        match cell {
            Data::Int(id) => {
                gauge_ids.insert(id.to_string());
            }
            Data::Float(id) => {
                gauge_ids.insert((*id as i64).to_string());
            }
            Data::String(s) if s.parse::<i64>().is_ok() => {
                gauge_ids.insert(s.clone());
            }
            _ => {}
        }
    }

    let mut gauge_list: Vec<_> = gauge_ids.into_iter().collect();
    gauge_list.sort();

    println!("\nTotal gauges found: {}", gauge_list.len());
    println!("\nFirst 20 gauges:");
    for id in gauge_list.iter().take(20) {
        println!("  {id}");
    }

    if gauge_list.len() > 20 {
        println!("\n... ({} more) ...", gauge_list.len() - 40);
        println!("\nLast 20 gauges:");
        for id in gauge_list.iter().rev().take(20).rev() {
            println!("  {id}");
        }
    }

    // Output all gauge IDs to a file for bulk checking
    println!("\nWriting all gauge IDs to /tmp/gauge_ids.txt");
    std::fs::write("/tmp/gauge_ids.txt", gauge_list.join("\n"))?;

    Ok(())
}
