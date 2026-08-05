use std::{io, fs, time::Duration, error::Error};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Tabs},
    Frame, Terminal,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Instant;
use dialoguer::Select;
use tempfile::NamedTempFile;
use chrono;

use crate::{
    check_setup_cmpl, debug_string, error_string, info_string, setup_string, trace_string, warning_string,
};
use crate::transport::ApiClient;
use crate::cache::{load_manifest, save_manifest};
use reqwest::Method;

// UI State
#[derive(Debug, Clone)]
struct AppState {
    current_tab: usize,
    character: Option<Character>,
    status_message: String,
    status_timer: Option<Instant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Character {
    name: String,
    race: Option<String>,
    classes: Vec<ClassLevel>,
    level: u8,
    background: Option<String>,
    alignment: String,
    abilities: Abilities,
    hit_points: HitPoints,
    skills: std::collections::HashMap<String, u8>,
    proficiencies: Vec<String>,
    inventory: Vec<Item>,
    features: Vec<Feature>,
    spells: Vec<Spell>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClassLevel {
    class: String,
    level: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Abilities {
    strength: u8,
    dexterity: u8,
    constitution: u8,
    intelligence: u8,
    wisdom: u8,
    charisma: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HitPoints {
    maximum: u16,
    current: i16,
    temporary: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Item {
    name: String,
    description: String,
    quantity: u16,
    weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Feature {
    name: String,
    description: String,
    source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Spell {
    name: String,
    level: u8,
    school: String,
    casting_time: String,
    range: String,
    components: String,
    duration: String,
    description: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            current_tab: 0,
            character: None,
            status_message: String::new(),
            status_timer: None,
        }
    }
}

impl AppState {
    fn set_status(&mut self, message: String) {
        self.status_message = message;
        self.status_timer = Some(Instant::now());
    }

    fn get_ability_modifier(&self, ability: u8) -> i8 {
        ((ability as i16 - 10) / 2) as i8
    }

    fn get_proficiency_bonus(&self) -> u8 {
        (self.character.as_ref().map_or(1, |c| c.level) as f32 / 4.0).ceil() as u8 + 1
    }
}

const TABS: &[&str] = &[
    "Character",
    "Abilities",
    "Skills",
    "Inventory",
    "Spells",
    "Features",
];

const OBJECT_TYPES: &[&str] = &[
    "Character",
    "Class",
    "Race",
    "Campaign",
    "Item",
    "Spell",
    "Feature",
    "Subclass",
];

#[derive(Debug)]
struct ObjectReference {
    id: String,
    object_type: String,
    name: String,
    synced: bool,
}

pub async fn run_ui() -> Result<(), Box<dyn Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = AppState::default();
    
    // Load character if exists
    if let Ok(character) = load_character() {
        app.character = Some(character);
    }

    // Main event loop. Keep terminal restoration outside the event loop so an
    // input/draw error cannot leave the user's shell in raw mode.
    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res.map_err(|err| -> Box<dyn Error> { Box::new(err) })
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut AppState,
) -> io::Result<()> {
    let tick_rate = Duration::from_millis(200);
    let mut last_tick = Instant::now();
    
    loop {
        terminal.draw(|f| ui(f, app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Right => {
                            app.current_tab = (app.current_tab + 1) % TABS.len();
                        }
                        KeyCode::Left => {
                            if app.current_tab > 0 {
                                app.current_tab -= 1;
                            } else {
                                app.current_tab = TABS.len() - 1;
                            }
                        }
                        KeyCode::Char('n') => {
                            if let Some(character) = &mut app.character {
                                if let Some(hp) = &mut character.hit_points.current.checked_add(1) {
                                    character.hit_points.current = *hp;
                                    app.set_status("HP increased".to_string());
                                }
                            }
                        }
                        KeyCode::Char('p') => {
                            if let Some(character) = &mut app.character {
                                if let Some(hp) = &mut character.hit_points.current.checked_sub(1) {
                                    character.hit_points.current = *hp;
                                    app.set_status("HP decreased".to_string());
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
            
            // Clear status message after 3 seconds
            if let Some(timer) = app.status_timer {
                if timer.elapsed() >= Duration::from_secs(3) {
                    app.status_message.clear();
                    app.status_timer = None;
                }
            }
        }
    }
}

fn ui(f: &mut Frame, app: &AppState) {
    let size = f.area();
    
    // Split the layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(3),  // Header
                Constraint::Min(10),    // Main content
                Constraint::Length(3),  // Tabs
                Constraint::Length(3),  // Status bar
            ]
            .as_ref(),
        )
        .split(size);

    // Header
    let title = match &app.character {
        Some(character) => format!("{} - Level {} {}", 
            character.name, 
            character.level,
            character.classes.first().map(|c| c.class.as_str()).unwrap_or("Adventurer")
        ),
        None => "D&D Character Manager".to_string(),
    };

    let title = Paragraph::new(Line::from(title))
        .style(Style::default().add_modifier(Modifier::BOLD))
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(title, chunks[0]);

    // Main content
    match app.current_tab {
        0 => draw_character_tab(f, app, chunks[1]),
        1 => draw_abilities_tab(f, app, chunks[1]),
        2 => draw_skills_tab(f, app, chunks[1]),
        3 => draw_inventory_tab(f, app, chunks[1]),
        4 => draw_spells_tab(f, app, chunks[1]),
        5 => draw_features_tab(f, app, chunks[1]),
        _ => {}
    }

    // Tabs
    const TABS: &[&str] = &["Character", "Abilities", "Skills", "Inventory", "Spells", "Features"];
    let tabs = Tabs::new(TABS.iter().map(|t| Line::from(*t)))
        .select(app.current_tab)
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow))
        .divider(" | ")
        .block(Block::default().borders(Borders::ALL).title("Navigation"));
    f.render_widget(tabs, chunks[2]);

    // Status bar
    let status = Paragraph::new(Line::from(app.status_message.clone()))
        .style(Style::default().fg(Color::LightBlue));
    f.render_widget(status, chunks[3]);
}

fn draw_character_tab(f: &mut Frame, app: &AppState, area: Rect) {
    if let Some(character) = &app.character {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .split(area);

        // Left side - Character info
        let info_block = Block::default()
            .title("Character Info")
            .borders(Borders::ALL);
        
        let mut info_text = vec![
            Line::from(format!("Name: {}", character.name)),
            Line::from(format!("Race: {}", character.race.as_deref().unwrap_or("Unknown"))),
            Line::from(format!(
                "Class: {}",
                character.classes.iter()
                    .map(|c| format!("{} {}", c.level, c.class))
                    .collect::<Vec<_>>()
                    .join(" / ")
            )),
            Line::from(format!("Background: {}", character.background.as_deref().unwrap_or("None"))),
            Line::from(format!("Alignment: {}", character.alignment)),
            Line::from(""),
            Line::from("Hit Points"),
            Line::from(format!("  Current: {}", character.hit_points.current)),
            Line::from(format!("  Maximum: {}", character.hit_points.maximum)),
            Line::from(format!("  Temporary: {}", character.hit_points.temporary))
        
        ];
        
        let info_paragraph = Paragraph::new(info_text).block(info_block);
        f.render_widget(info_paragraph, chunks[0]);
        
        // Right side - Stats
        let stats_block = Block::default()
            .title("Stats")
            .borders(Borders::ALL);
            
        let stats_text = vec![
            Line::from("Press 'n' to increase HP, 'p' to decrease"),
            Line::from(""),
        ];
        
        let stats_paragraph = Paragraph::new(stats_text).block(stats_block);
        f.render_widget(stats_paragraph, chunks[1]);
    } else {
        let block = Block::default()
            .title("No Character Loaded")
            .borders(Borders::ALL);
        
        let text = vec![
            Line::from("No character is currently loaded."),
            Line::from("Create a new character or load an existing one."),
        ];
        
        let paragraph = Paragraph::new(text)
            .block(block)
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(paragraph, area);
    }
}

fn draw_abilities_tab(f: &mut Frame, app: &AppState, area: Rect) {
    if let Some(character) = &app.character {
        let ability_scores = [
            ("Strength", character.abilities.strength),
            ("Dexterity", character.abilities.dexterity),
            ("Constitution", character.abilities.constitution),
            ("Intelligence", character.abilities.intelligence),
            ("Wisdom", character.abilities.wisdom),
            ("Charisma", character.abilities.charisma),
        ];

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Length(3),
                    Constraint::Min(0),
                ]
                .as_ref(),
            )
            .split(area);

        // Draw ability scores in a grid (2 columns)
        let ability_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Percentage(50),
                    Constraint::Percentage(50),
                ]
                .as_ref(),
            )
            .split(chunks[1]);

        // Draw ability scores in two columns
        for col in 0..2 {
            let ability_block = Block::default()
                .borders(Borders::ALL)
                .title(if col == 0 { "Abilities" } else { " " });
            
            let mut ability_text = Vec::new();
            for row in 0..3 {
                let idx = row * 2 + col;
                if idx >= ability_scores.len() {
                    break;
                }
                let (name, score) = ability_scores[idx];
                let modifier = (score as i16 - 10) / 2;
                ability_text.push(Line::from(
                format!("{}: {} ({}{})", 
                    name, 
                    score,
                    if modifier >= 0 { "+" } else { "" },
                    modifier
                )
            ));
            }
                
            let ability_paragraph = Paragraph::new(ability_text).block(ability_block);
            f.render_widget(ability_paragraph, ability_chunks[col]);
        }
    }
}

// Stub functions for other tabs
fn draw_skills_tab(f: &mut Frame, _app: &AppState, area: Rect) {
    let block = Block::default()
        .title("Skills")
        .borders(Borders::ALL);
    let text = vec![Line::from("Skills tab content")];
    let paragraph = Paragraph::new(text).block(block);
    f.render_widget(paragraph, area);
}

fn draw_inventory_tab(f: &mut Frame, _app: &AppState, area: Rect) {
    let block = Block::default()
        .title("Inventory")
        .borders(Borders::ALL);
    let text = vec![Line::from("Inventory tab content")];
    let paragraph = Paragraph::new(text).block(block);
    f.render_widget(paragraph, area);
}

fn draw_spells_tab(f: &mut Frame, _app: &AppState, area: Rect) {
    let block = Block::default()
        .title("Spells")
        .borders(Borders::ALL);
    let text = vec![Line::from("Spells tab content")];
    let paragraph = Paragraph::new(text).block(block);
    f.render_widget(paragraph, area);
}

fn draw_features_tab(f: &mut Frame, _app: &AppState, area: Rect) {
    let block = Block::default()
        .title("Features & Traits")
        .borders(Borders::ALL);
    let text = vec![Line::from("Features tab content")];
    let paragraph = Paragraph::new(text).block(block);
    f.render_widget(paragraph, area);
}

fn load_character() -> Result<Character, Box<dyn Error>> {
    // TODO: Implement actual character loading from file
    Ok(Character {
        name: "Erevan Moonshadow".to_string(),
        race: Some("High Elf".to_string()),
        classes: vec![ClassLevel {
            class: "Wizard".to_string(),
            level: 5,
        }],
        level: 5,
        background: Some("Sage".to_string()),
        alignment: "Neutral Good".to_string(),
        abilities: Abilities {
            strength: 8,
            dexterity: 16,
            constitution: 14,
            intelligence: 18,
            wisdom: 12,
            charisma: 10,
        },
        hit_points: HitPoints {
            maximum: 32,
            current: 28,
            temporary: 0,
        },
        skills: std::collections::HashMap::new(),
        proficiencies: vec!["Arcana".to_string(), "Investigation".to_string()],
        inventory: Vec::new(),
        features: Vec::new(),
        spells: Vec::new(),
    })
}

pub async fn create_object(object_type: Option<&str>) -> Result<(), Box<dyn Error>> {
    // For now, we'll just create a default character since we're focusing on the TUI
    let character = Character {
        name: "New Character".to_string(),
        race: None,
        classes: Vec::new(),
        level: 1,
        background: None,
        alignment: "Neutral".to_string(),
        abilities: Abilities {
            strength: 10,
            dexterity: 10,
            constitution: 10,
            intelligence: 10,
            wisdom: 10,
            charisma: 10,
        },
        hit_points: HitPoints {
            maximum: 10,
            current: 10,
            temporary: 0,
        },
        skills: std::collections::HashMap::new(),
        proficiencies: Vec::new(),
        inventory: Vec::new(),
        features: Vec::new(),
        spells: Vec::new(),
    };

    // Save the character
    let save_path = crate::config::data_dir()?
        .join("saved_objs")
        .join("character.json");
    
    fs::write(&save_path, serde_json::to_string_pretty(&character)?)?;
    println!("{} Created new character at: {}", info_string("✓"), save_path.display());
    
    Ok(())
}

pub async fn edit_object(object_type: Option<&str>, object_id: Option<&str>) -> Result<(), Box<dyn Error>> {
    let object_type = match object_type {
        Some(t) => t,
        None => {
            let selection = dialoguer::Select::new()
                .with_prompt("Select object type to edit")
                .items(OBJECT_TYPES)
                .interact()?;
            OBJECT_TYPES[selection]
        }
    };

    let object_id = match object_id {
        Some(id) => id.to_string(),
        None => {
            // List objects of the selected type and let user choose one
            let objects = list_objects_in_cache(object_type)?;
            if objects.is_empty() {
                return Err(anyhow::anyhow!("No {} objects found to edit", object_type).into());
            }
            
            let selection = dialoguer::Select::new()
                .with_prompt(format!("Select {} to edit", object_type))
                .items(&objects)
                .interact()?;
            objects[selection].clone()
        }
    };

    // Open the object in the default editor
    let file_path = crate::config::data_dir()?
        .join("saved_objs")
        .join(object_type.to_lowercase())
        .join(format!("{}.json", object_id));

    if !file_path.exists() {
        return Err(anyhow::anyhow!("File not found: {}", file_path.display()).into());
    }

    // Create a temporary file for editing
    let temp_file = tempfile::NamedTempFile::new()?;
    let temp_path = temp_file.path().to_owned();
    std::fs::copy(&file_path, &temp_path)?;

    // Open the editor
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
    let status = std::process::Command::new(editor)
        .arg(&temp_path)
        .status()?;

    if !status.success() {
        return Err(anyhow::anyhow!("Editor exited with status: {}", status).into());
    }

    // Validate JSON before saving
    let content = std::fs::read_to_string(&temp_path)?;
    let _: Value = serde_json::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Invalid JSON: {}", e))?;

    // Save the changes
    std::fs::create_dir_all(file_path.parent().unwrap())?;
    std::fs::copy(temp_path, &file_path)?;

    println!("{} Successfully updated {} {}", info_string("✓"), object_type, object_id);
    Ok(())
}

pub async fn delete_object(object_type: Option<&str>, object_id: Option<&str>) -> Result<(), Box<dyn Error>> {
    let object_type = match object_type {
        Some(t) => t,
        None => {
            let selection = dialoguer::Select::new()
                .with_prompt("Select object type to delete")
                .items(OBJECT_TYPES)
                .interact()?;
            OBJECT_TYPES[selection]
        }
    };

    let object_id = match object_id {
        Some(id) => id.to_string(),
        None => {
            let objects = list_objects_in_cache(object_type)?;
            if objects.is_empty() {
                return Err(anyhow::anyhow!("No {} objects found to delete", object_type).into());
            }
            
            let selection = dialoguer::Select::new()
                .with_prompt(format!("Select {} to delete", object_type))
                .items(&objects)
                .interact()?;
            objects[selection].clone()
        }
    };

    // Confirm deletion
    if !dialoguer::Confirm::new()
        .with_prompt(format!("Are you sure you want to delete {} {}?", object_type, object_id))
        .interact()? {
        return Ok(());
    }

    // Delete the file
    let file_path = crate::config::data_dir()?
        .join("saved_objs")
        .join(object_type.to_lowercase())
        .join(format!("{}.json", object_id));

    if file_path.exists() {
        std::fs::remove_file(&file_path)?;
        println!("{} Successfully deleted {} {}", info_string("✓"), object_type, object_id);
    } else {
        return Err(anyhow::anyhow!("File not found: {}", file_path.display()).into());
    }

    Ok(())
}

pub async fn list_objects(filter: Option<&str>) -> Result<(), Box<dyn Error>> {
    let object_type = match filter {
        Some(t) if OBJECT_TYPES.contains(&t) => t,
        _ => {
            let selection = dialoguer::Select::new()
                .with_prompt("Select object type to list")
                .items(OBJECT_TYPES)
                .interact()?;
            OBJECT_TYPES[selection]
        }
    };

    let objects = list_objects_in_cache(object_type)?;
    if objects.is_empty() {
        println!("No {} objects found", object_type);
        return Ok(());
    }

    println!("\n{} {}", info_string("✓"), object_type);
    println!("{}", "-".repeat(50));
    for obj in &objects {
        println!("- {}", obj);
    }
    println!("\nTotal: {}\n", objects.len());

    Ok(())
}

pub async fn sync_objects() -> Result<(), Box<dyn Error>> {
    // Check if user is logged in
    let token_path = crate::config::data_dir()?.join(".auth_tokens.txt");
    
    if !token_path.exists() {
        return Err(anyhow::anyhow!("Not logged in. Please log in first.").into());
    }

    // Get the user ID
    let _user_id = std::fs::read_to_string(&token_path)?.lines().nth(2).unwrap_or_default().to_string();
    
    // Sync each object type
    for &object_type in OBJECT_TYPES {
        let objects = list_objects_in_cache(object_type)?;
        if objects.is_empty() {
            continue;
        }

        println!("\nSyncing {} {}...", objects.len(), object_type);
        
        for obj_id in objects {
            let result = ready_resource(obj_id.clone(), object_type.to_string()).await;
            match result {
                Ok(_) => println!("  {} Synced {} {}", info_string("✓"), object_type, obj_id),
                Err(e) => println!("  {} Failed to sync {} {}: {}", error_string("✗"), object_type, obj_id, e),
            }
        }
    }

    // Update sync timestamp
    let sync_file = crate::config::data_dir()?.join("synced.txt");
    let now = chrono::Utc::now().to_rfc3339();
    std::fs::write(sync_file, &now)?;

    println!("\n{} Sync completed at {}\n", info_string("✓"), now);
    Ok(())
}

// Helper function to list objects of a specific type from the cache
fn list_objects_in_cache(object_type: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let dir_path = crate::config::data_dir()?
        .join("saved_objs")
        .join(object_type.to_lowercase());

    if !dir_path.exists() {
        return Ok(Vec::new());
    }

    let mut objects = Vec::new();
    for entry in std::fs::read_dir(dir_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "json") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                objects.push(stem.to_string());
            }
        }
    }

    objects.sort();
    Ok(objects)
}

pub async fn get_object(object_type: Option<&str>, object_id: Option<&str>) -> Result<(), Box<dyn Error>> {
    let object_type = match object_type {
        Some(t) => t,
        None => {
            let selection = dialoguer::Select::new()
                .with_prompt("Select object type to fetch")
                .items(OBJECT_TYPES)
                .interact()?;
            OBJECT_TYPES[selection]
        }
    };

    let object_id = match object_id {
        Some(id) => id.to_string(),
        None => {
            println!("Enter {} ID to fetch:", object_type);
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            input.trim().to_string()
        }
    };

    if object_id.is_empty() {
        return Err(anyhow::anyhow!("Object ID cannot be empty").into());
    }

    // Check if already exists locally
    let local_path = crate::config::data_dir()?
        .join("saved_objs")
        .join(object_type.to_lowercase())
        .join(format!("{}.json", object_id));

    if local_path.exists() {
        println!("Checking local {} {} cache for this session...", object_type, object_id);
    } else {
        println!("Fetching {} {} from server...", object_type, object_id);
    }
    ready_resource(object_id.clone(), object_type.to_string()).await?;
    
    println!("{} Successfully fetched {} {}", info_string("✓"), object_type, object_id);
    Ok(())
}

pub async fn view_object(object_type: Option<&str>, object_id: Option<&str>) -> Result<(), Box<dyn Error>> {
    let object_type = match object_type {
        Some(t) => t,
        None => {
            let selection = dialoguer::Select::new()
                .with_prompt("Select object type to view")
                .items(OBJECT_TYPES)
                .interact()?;
            OBJECT_TYPES[selection]
        }
    };

    let object_id = match object_id {
        Some(id) => id.to_string(),
        None => {
            let objects = list_objects_in_cache(object_type)?;
            if objects.is_empty() {
                return Err(anyhow::anyhow!("No {} objects found to view", object_type).into());
            }
            
            let selection = dialoguer::Select::new()
                .with_prompt(format!("Select {} to view", object_type))
                .items(&objects)
                .interact()?;
            objects[selection].clone()
        }
    };

    // Read and display the object
    let file_path = crate::config::data_dir()?
        .join("saved_objs")
        .join(object_type.to_lowercase())
        .join(format!("{}.json", object_id));

    if !file_path.exists() {
        return Err(anyhow::anyhow!("File not found: {}", file_path.display()).into());
    }

    let content = std::fs::read_to_string(&file_path)?;
    let value: Value = serde_json::from_str(&content)?;
    
    // Pretty print the JSON
    println!("\n=== {} {} ===\n", object_type, object_id);
    println!("{}", serde_json::to_string_pretty(&value)?);
    println!("\n=== End of {} {} ===\n", object_type, object_id);

    Ok(())
}

fn show_help() {
    // Help is now shown in the TUI itself
}

async fn load_cache() -> Result<(Vec<String>), anyhow::Error> {
    let saved_objs_dir = crate::config::data_dir()?.join("saved_objs");

    if !saved_objs_dir.exists() {
        std::fs::create_dir_all(&saved_objs_dir)?;
    }

    // Load the cache from the saved_objs directory
    // This could involve reading files, parsing JSON, etc.
    
    Ok(vec![]) // Placeholder for actual cache loading logic
}

async fn ready_resource(resource_id: String, resource_type: String) -> Result<(), anyhow::Error> {
    if resource_id.is_empty() || resource_type.is_empty() {
        return Err(anyhow::anyhow!("Resource ID or Type cannot be empty."));
    }
    
    let data_root = crate::config::data_dir()?;
    let resource_type = resource_type.to_lowercase();
    let cache_key = format!("{}/{}", resource_type, resource_id);
    let session_id = crate::auth::current_client_session_id()?;
    let mut manifest = load_manifest(&data_root)?;

    if search_cache(&resource_type, &resource_id)?
        && manifest.get(&cache_key).map(String::as_str) == Some(session_id.as_str())
    {
        return Ok(());
    }

    // Make a retry-aware GET request only once per resource in the active
    // client session. The response is persisted before the manifest is
    // updated, so an interrupted write cannot create a false cache hit.
    let client = ApiClient::from_saved_profile()?;
    let path = format!("/func/{}/{}", resource_type.to_lowercase(), resource_id);
    let (access_token, _) = crate::auth::read_auth_tokens()?;
    let response = client
        .send_json_with_retry(Method::GET, &path, Some(&access_token), None)
        .await?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Failed to fetch resource: {}", response.status()));
    }

    let resource_data = response.text().await?;
    let _: Value = serde_json::from_str(&resource_data)
        .map_err(|error| anyhow::anyhow!("Server returned invalid JSON for cached resource: {error}"))?;
    let res_dir = data_root.join("saved_objs").join(&resource_type);
    fs::create_dir_all(&res_dir)?;
    let final_path = res_dir.join(format!("{}.json", resource_id));
    let temp_path = res_dir.join(format!(".{}.json.tmp", resource_id));
    fs::write(&temp_path, resource_data)?;
    fs::rename(&temp_path, &final_path)?;
    manifest.insert(cache_key, session_id);
    save_manifest(&data_root, &manifest)?;

    Ok(())

}

fn search_cache(resource_type: &str, resource_id: &str) -> Result<bool, anyhow::Error> {
    if resource_type.is_empty() || resource_id.is_empty() {
        return Err(anyhow::anyhow!("Resource type or ID cannot be empty."));
    }

    let resource_path = crate::config::data_dir()?.join("saved_objs").join(resource_type.to_lowercase()).join(format!("{}.json", resource_id));

    Ok(resource_path.exists())
}
