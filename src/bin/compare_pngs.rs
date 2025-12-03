use std::fs::File;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <png1> <png2>", args[0]);
        std::process::exit(1);
    }

    let file1 = File::open(&args[1]).expect("Failed to open first file");
    let file2 = File::open(&args[2]).expect("Failed to open second file");

    let decoder1 = png::Decoder::new(file1);
    let decoder2 = png::Decoder::new(file2);

    let mut reader1 = decoder1.read_info().expect("Failed to read first PNG");
    let mut reader2 = decoder2.read_info().expect("Failed to read second PNG");

    let (width1, height1, ct1) = {
        let info = reader1.info();
        (info.width as usize, info.height as usize, format!("{:?}", info.color_type))
    };
    let (width2, height2, ct2) = {
        let info = reader2.info();
        (info.width as usize, info.height as usize, format!("{:?}", info.color_type))
    };

    println!("PNG 1: {}x{}, {}", width1, height1, ct1);
    println!("PNG 2: {}x{}, {}", width2, height2, ct2);

    let mut buf1 = vec![0; reader1.output_buffer_size()];
    let mut buf2 = vec![0; reader2.output_buffer_size()];

    reader1.next_frame(&mut buf1).expect("Failed to read first PNG data");
    reader2.next_frame(&mut buf2).expect("Failed to read second PNG data");

    println!("Data size 1: {}", buf1.len());
    println!("Data size 2: {}", buf2.len());

    let mut diff_count = 0;
    let mut first_diffs = Vec::new();

    let width = width1;

    for i in 0..buf1.len().min(buf2.len()) {
        if buf1[i] != buf2[i] {
            if first_diffs.len() < 20 {
                let x = i % width;
                let y = i / width;
                first_diffs.push((x, y, buf1[i], buf2[i]));
            }
            diff_count += 1;
        }
    }

    println!("\nTotal different pixels: {}", diff_count);
    println!("First {} differences:", first_diffs.len());
    for (x, y, v1, v2) in &first_diffs {
        println!("  ({}, {}): {} vs {}", x, y, v1, v2);
    }

    let nonzero1: usize = buf1.iter().filter(|&&b| b != 0).count();
    let nonzero2: usize = buf2.iter().filter(|&&b| b != 0).count();
    println!("\nNon-zero pixels in test: {}", nonzero1);
    println!("Non-zero pixels in orig: {}", nonzero2);

    // Sample some specific chunks
    println!("\nChunk 0 (0,0) first 32x32 pixels:");
    let chunk_size = 32;
    println!("Test first row: {:?}", &buf1[0..chunk_size]);
    println!("Orig first row: {:?}", &buf2[0..chunk_size]);
}
