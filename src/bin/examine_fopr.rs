use calamine::{open_workbook_auto, DataType, Reader};
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let file_path = if args.len() > 1 {
        &args[1]
    } else {
        "sample-data-files/59700_FOPR.xlsx"
    };

    println!("Opening FOPR file: {file_path}");
    let mut workbook = open_workbook_auto(file_path)?;

    println!("\nSheet names:");
    for (i, name) in workbook.sheet_names().iter().enumerate() {
        println!("  {i}: {name}");
    }

    // Allow specifying which sheet to examine
    let sheet_name = if args.len() > 2 {
        args[2].clone()
    } else {
        // Default to examining a year sheet (2024)
        "2024".to_string()
    };

    println!("\n\nExamining sheet: {sheet_name}");
    println!("{}", "=".repeat(100));

    let range = workbook.worksheet_range(&sheet_name)?;

    println!("Dimensions: {:?}", range.get_size());
    println!("\nFirst 40 rows (showing first 10 columns):");
    println!("{}", "=".repeat(100));

    for (row_idx, row) in range.rows().enumerate().take(40) {
        // Only print rows with data
        let has_data = row.iter().any(|cell| !cell.is_empty());
        if has_data {
            print!("Row {:3}: ", row_idx + 1);
            for cell in row.iter().take(10) {
                if cell.is_empty() {
                    print!("[empty] ");
                } else {
                    print!("[{cell}] ");
                }
            }
            println!();
        }
    }

    // Also show a sample of the full width to understand structure
    println!("\n{}", "=".repeat(100));
    println!("Sample of full row width (row 5, all columns):");
    println!("{}", "=".repeat(100));
    if let Some(row) = range.rows().nth(4) {
        for (col_idx, cell) in row.iter().enumerate() {
            if !cell.is_empty() {
                println!("Col {:3}: {}", col_idx + 1, cell);
            }
        }
    }

    Ok(())
}
