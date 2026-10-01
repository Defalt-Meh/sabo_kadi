//! Generate a tiny sample workbook for trying out the importer.
//!
//! ```text
//! cargo run --example generate_sample_xlsx -- data/sample.xlsx
//! cargo run --bin import_xlsx -- data/sample.xlsx
//! ```

use rust_xlsxwriter::Workbook;

const HEADERS: &[&str] = &[
    "doc_id",
    "degree",
    "position type",
    "period",
    "salary",
    "old_salary",
    "asitane",
    "infisal",
    "old_kadi",
    "new_kadi",
    "old_place",
    "new_place",
    "tarih",
    "varak_no",
    "region",
    "certificate",
    "text",
    "old_latitude",
    "old_longitude",
    "new_latitude",
    "new_longitude",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "data/sample.xlsx".to_string());

    // doc_id, old_kadi, new_kadi, old_place, new_place, tarih, region, text,
    // (old_lat, old_lon, new_lat, new_lon)
    type Row = (
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        Option<(f64, f64, f64, f64)>,
    );

    let rows: &[Row] = &[
        (
            "KA-0001",
            "es-Seyyid Ahmed Efendi",
            "Mehmed Emin Efendi",
            "Selanik",
            "Manastır",
            "1123",
            "Rumeli",
            "ber-vech-i arpalık Manastır kazası tevcih olundu",
            Some((40.6403, 22.9439, 41.0314, 21.3347)),
        ),
        (
            "KA-0002",
            "Mehmed Emin Efendi",
            "İbrahim Efendi",
            "Manastır",
            "Yenişehir-i Fener",
            "Ramazan 1125",
            "Rumeli",
            "azl ile Yenişehir kazası ibka buyuruldu",
            None,
        ),
        (
            "KA-0003",
            "İbrahim Efendi",
            "es-Seyyid Ahmed Efendi",
            "Yenişehir-i Fener",
            "Selanik",
            "1130",
            "Rumeli",
            "-",
            None,
        ),
        (
            "KA-0004",
            "Osman Nuri Efendi",
            "es-Seyyid Ahmed Efendi",
            "Kesriye",
            "Selanik",
            "1131",
            "Rumeli",
            "Selanik mevleviyeti tevcih olundu",
            Some((40.5150, 21.2680, 40.6403, 22.9439)),
        ),
    ];

    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.set_name("Atamalar")?;

    for (c, h) in HEADERS.iter().enumerate() {
        sheet.write(0, c as u16, *h)?;
    }

    for (r, row) in rows.iter().enumerate() {
        let excel_row = (r + 1) as u32;
        let (doc_id, old_kadi, new_kadi, old_place, new_place, tarih, region, text, coords) = *row;
        sheet.write(excel_row, 0, doc_id)?;
        sheet.write(excel_row, 1, "Mevleviyet")?; // degree
        sheet.write(excel_row, 2, "kaza")?; // position type
        sheet.write(excel_row, 8, old_kadi)?;
        sheet.write(excel_row, 9, new_kadi)?;
        sheet.write(excel_row, 10, old_place)?;
        sheet.write(excel_row, 11, new_place)?;
        sheet.write(excel_row, 12, tarih)?;
        sheet.write(excel_row, 14, region)?;
        sheet.write(excel_row, 16, text)?;
        if let Some((ola, olo, nla, nlo)) = coords {
            sheet.write(excel_row, 17, ola)?;
            sheet.write(excel_row, 18, olo)?;
            sheet.write(excel_row, 19, nla)?;
            sheet.write(excel_row, 20, nlo)?;
        }
    }

    workbook.save(&path)?;
    println!("wrote {path} ({} data rows)", rows.len());
    Ok(())
}
