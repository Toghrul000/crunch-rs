use clap::{Arg, Command};
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const DIGITS: &str = "0123456789";

#[derive(Debug)]
struct Config {
    min_len: usize,
    max_len: usize,
    charset: String,
    template: Option<String>,
    output: Option<String>,
    no_duplicates: bool,
}

struct Progress {
    current: Arc<AtomicU64>,
    total: u64,
    last_percentage: Arc<AtomicU64>,
}

impl Progress {
    fn new(total: u64) -> Self {
        Progress {
            current: Arc::new(AtomicU64::new(0)),
            total,
            last_percentage: Arc::new(AtomicU64::new(0)),
        }
    }

    fn increment(&self) {
        let current = self.current.fetch_add(1, Ordering::SeqCst) + 1;
        let percentage = (current as f64 / self.total as f64 * 100.0) as u64;
        let last_percentage = self.last_percentage.load(Ordering::SeqCst);

        if percentage >= last_percentage + 5 {
            self.last_percentage.store(percentage, Ordering::SeqCst);
            println!("{}% done", percentage);
        }
    }
}

fn has_consecutive_duplicates(word: &str) -> bool {
    let chars: Vec<char> = word.chars().collect();
    for i in 0..chars.len() - 1 {
        // Skip checking digits
        if !chars[i].is_digit(10) && !chars[i + 1].is_digit(10) {
            if chars[i] == chars[i + 1] {
                return true;
            }
        }
    }
    false
}

fn calculate_template_size_no_duplicates(template: &str, charset: &str) -> u64 {
    let mut total: u64 = 1;
    let mut last_was_char = false;
    
    for (_i, c) in template.chars().enumerate() {
        match c {
            '@' => {
                if last_was_char {
                    // If previous position was also a character,
                    // we can't use the same character as the previous position
                    total *= charset.len() as u64 - 1;
                } else {
                    // If previous position was not a character (or first position),
                    // we can use any character
                    total *= charset.len() as u64;
                }
                last_was_char = true;
            }
            '%' => {
                // For digits, we can always use all possibilities
                total *= 10;
                last_was_char = false;
            }
            _ => {
                last_was_char = false;
            }
        }
    }
    total
}

fn calculate_combinations_no_duplicates(length: u32, charset_len: u32) -> u64 {
    if length == 0 {
        return 1;
    }
    if length == 1 {
        return charset_len as u64;
    }

    // For each position after the first:
    // - If we use a different character than the previous position, we have (charset_len - 1) choices
    // First position can use any character (charset_len)
    let mut total = charset_len as u64;
    for _ in 1..length {
        total *= (charset_len - 1) as u64;
    }
    
    total
}

fn calculate_size(config: &Config) -> u64 {
    if let Some(template) = &config.template {
        if config.no_duplicates {
            calculate_template_size_no_duplicates(template, &config.charset)
        } else {
            let char_positions = template.chars().filter(|&c| c == '@').count();
            let num_positions = template.chars().filter(|&c| c == '%').count();
            
            let char_combinations = config.charset.len().pow(char_positions as u32);
            let num_combinations = 10u64.pow(num_positions as u32);
            
            char_combinations as u64 * num_combinations
        }
    } else {
        if config.no_duplicates {
            let mut total = 0u64;
            for len in config.min_len..=config.max_len {
                total += calculate_combinations_no_duplicates(len as u32, config.charset.len() as u32);
            }
            total
        } else {
            let charset_len = config.charset.len() as u64;
            let mut total = 0u64;
            for len in config.min_len..=config.max_len {
                total += charset_len.pow(len as u32);
            }
            total
        }
    }
}

fn format_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    let size_with_newlines = size * 8; // Approximate average line length
    if size_with_newlines >= GB {
        format!("{:.2} GB", size_with_newlines as f64 / GB as f64)
    } else if size_with_newlines >= MB {
        format!("{:.2} MB", size_with_newlines as f64 / MB as f64)
    } else if size_with_newlines >= KB {
        format!("{:.2} KB", size_with_newlines as f64 / KB as f64)
    } else {
        format!("{} B", size_with_newlines)
    }
}

fn generate_from_template<W: Write>(
    config: &Config, 
    template: &str, 
    writer: &mut W,
    progress: &Progress,
) -> io::Result<()> {
    let positions: Vec<(usize, char)> = template
        .chars()
        .enumerate()
        .filter(|&(_, c)| c == '@' || c == '%')
        .collect();

    let mut current = vec![0; positions.len()];
    let mut word = template.to_string();

    loop {
        // Create the word based on current indices
        for (pos_idx, (template_idx, template_char)) in positions.iter().enumerate() {
            let charset = if *template_char == '@' {
                &config.charset
            } else {
                DIGITS
            };
            let char_idx = current[pos_idx];
            word.replace_range(
                *template_idx..*template_idx + 1,
                &charset.chars().nth(char_idx).unwrap().to_string(),
            );
        }

        if !config.no_duplicates || !has_consecutive_duplicates(&word) {
            writeln!(writer, "{}", word)?;
        }
        progress.increment();

        // Increment indices
        let mut idx = positions.len() - 1;
        loop {
            let charset_len = if positions[idx].1 == '@' {
                config.charset.len()
            } else {
                DIGITS.len()
            };

            current[idx] += 1;
            if current[idx] < charset_len {
                break;
            }

            current[idx] = 0;
            if idx == 0 {
                return Ok(());
            }
            idx -= 1;
        }
    }
}

fn generate_all_combinations<W: Write>(
    current: &mut String,
    length: usize,
    charset: &str,
    writer: &mut W,
    progress: &Progress,
    no_duplicates: bool,
) -> io::Result<()> {
    if length == 0 {
        if !no_duplicates || !has_consecutive_duplicates(current) {
            writeln!(writer, "{}", current)?;
        }
        progress.increment();
        return Ok(());
    }

    for c in charset.chars() {
        if no_duplicates && !current.is_empty() {
            let last_char = current.chars().last().unwrap();
            // Allow duplicate digits
            if c == last_char && !c.is_digit(10) {
                continue;
            }
        }
        current.push(c);
        generate_all_combinations(current, length - 1, charset, writer, progress, no_duplicates)?;
        current.pop();
    }
    Ok(())
}

fn generate_words<W: Write>(
    config: &Config, 
    writer: &mut W,
    progress: &Progress,
) -> io::Result<()> {
    if let Some(template) = &config.template {
        generate_from_template(config, template, writer, progress)?;
    } else {
        let mut current = String::new();
        for len in config.min_len..=config.max_len {
            generate_all_combinations(
                &mut current, 
                len, 
                &config.charset, 
                writer, 
                progress, 
                config.no_duplicates
            )?;
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let matches = Command::new("crunch-rs")
        .version("1.0")
        .author("lurg0th")
        .about("A Rust clone of the crunch wordlist generator")
        .arg(
            Arg::new("min_len")
                .required(true)
                .help("Minimum length of generated words"),
        )
        .arg(
            Arg::new("max_len")
                .required(true)
                .help("Maximum length of generated words"),
        )
        .arg(
            Arg::new("charset")
                .required(true)
                .help("Characters to use in generation"),
        )
        .arg(
            Arg::new("template")
                .short('t')
                .long("template")
                .help("Template for generation (@ for charset, % for digits)"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .help("Output file name"),
        )
        .arg(
            Arg::new("no-duplicates")
                .long("no-duplicates")
                .action(clap::ArgAction::SetTrue)
                .help("Avoid consecutive duplicate characters (except digits)"),
        )
        .get_matches();

    let config = Config {
        min_len: matches
            .get_one::<String>("min_len")
            .unwrap()
            .parse()
            .expect("Invalid minimum length"),
        max_len: matches
            .get_one::<String>("max_len")
            .unwrap()
            .parse()
            .expect("Invalid maximum length"),
        charset: matches.get_one::<String>("charset").unwrap().to_string(),
        template: matches.get_one::<String>("template").cloned(),
        output: matches.get_one::<String>("output").cloned(),
        no_duplicates: matches.get_flag("no-duplicates"),
    };

    let total_combinations = calculate_size(&config);
    println!("Will create approx: {} ({} combinations)", format_size(total_combinations), total_combinations);
    
    let progress = Progress::new(total_combinations);
    println!("0% done");

    if let Some(output) = &config.output {
        let file = File::create(Path::new(output))?;
        let mut writer = BufWriter::new(file);
        generate_words(&config, &mut writer, &progress)?;
    } else {
        let mut stdout = io::stdout();
        generate_words(&config, &mut stdout, &progress)?;
    }

    println!("100% done");
    Ok(())
}