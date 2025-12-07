//! HIVE Commander - Intent-Based Command TUI
//!
//! A turn-based tactical game demonstrating HIVE's hierarchical command concept:
//! - Commander sees COMPOSED CAPABILITIES, not individual pieces
//! - Commander sets INTENT through objectives
//! - HIVE generates COURSES OF ACTION (COAs)
//! - Pieces execute AUTONOMOUSLY based on chosen COA
//!
//! Run with: cargo run -p hive-commander

// This is a prototype - allow unused code for now
#![allow(dead_code)]
#![allow(clippy::upper_case_acronyms)]

mod protocol_types;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use noise::{NoiseFn, Perlin};
use rand::Rng;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::io::{self, stdout};

// =============================================================================
// TERRAIN
// =============================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
enum Terrain {
    DeepWater,
    ShallowWater,
    Plains,
    Forest,
    Hills,
    Mountain,
    Urban,
    Base,
}

impl Terrain {
    fn elevation(&self) -> i32 {
        match self {
            Terrain::DeepWater => -1,
            Terrain::ShallowWater => 0,
            Terrain::Plains => 0,
            Terrain::Forest => 0,
            Terrain::Hills => 1,
            Terrain::Mountain => 2,
            Terrain::Urban => 0,
            Terrain::Base => 0,
        }
    }

    fn symbol(&self) -> &'static str {
        match self {
            Terrain::DeepWater => "≈",
            Terrain::ShallowWater => "~",
            Terrain::Plains => "·",
            Terrain::Forest => "♣",
            Terrain::Hills => "^",
            Terrain::Mountain => "▲",
            Terrain::Urban => "▣",
            Terrain::Base => "◉",
        }
    }

    fn style(&self) -> Style {
        match self {
            Terrain::DeepWater => Style::default().fg(Color::Blue).bg(Color::Rgb(20, 20, 40)),
            Terrain::ShallowWater => Style::default().fg(Color::Cyan).bg(Color::Rgb(30, 30, 50)),
            Terrain::Plains => Style::default().fg(Color::Rgb(80, 100, 60)),
            Terrain::Forest => Style::default().fg(Color::Rgb(0, 80, 0)),
            Terrain::Hills => Style::default().fg(Color::Rgb(180, 160, 100)),
            Terrain::Mountain => Style::default().fg(Color::White).bg(Color::Rgb(60, 60, 60)),
            Terrain::Urban => Style::default().fg(Color::Rgb(120, 120, 120)),
            Terrain::Base => Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        }
    }

    fn movement_cost(&self) -> Option<i32> {
        match self {
            Terrain::DeepWater => None,
            Terrain::ShallowWater => Some(2),
            Terrain::Plains => Some(1),
            Terrain::Forest => Some(2),
            Terrain::Hills => Some(2),
            Terrain::Mountain => None,
            Terrain::Urban => Some(1),
            Terrain::Base => Some(1),
        }
    }
}

// =============================================================================
// DETECTION MODES
// =============================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
enum DetectionMode {
    EO,
    IR,
    Radar,
    Acoustic,
    SIGINT,
}

impl DetectionMode {
    fn name(&self) -> &'static str {
        match self {
            DetectionMode::EO => "EO",
            DetectionMode::IR => "IR",
            DetectionMode::Radar => "RAD",
            DetectionMode::Acoustic => "ACO",
            DetectionMode::SIGINT => "SIG",
        }
    }

    fn range(&self) -> i32 {
        match self {
            DetectionMode::EO => 4,
            DetectionMode::IR => 5,
            DetectionMode::Radar => 8,
            DetectionMode::Acoustic => 6,
            DetectionMode::SIGINT => 10,
        }
    }
}

// =============================================================================
// CAPABILITY CLASSES (Individual Pieces)
// =============================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
enum PieceType {
    Sensor(DetectionMode),
    Scout,
    Striker,
    Support,
    Authority,
}

impl PieceType {
    fn symbol(&self) -> String {
        match self {
            PieceType::Sensor(mode) => format!("S{}", &mode.name()[0..1].to_lowercase()),
            PieceType::Scout => "Rc".to_string(),
            PieceType::Striker => "St".to_string(),
            PieceType::Support => "Su".to_string(),
            PieceType::Authority => "Au".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Team {
    Blue,
    Red,
}

#[derive(Clone, Debug)]
struct Piece {
    id: usize,
    piece_type: PieceType,
    team: Team,
    x: usize,
    y: usize,
    fuel: i32,
    max_fuel: i32,
}

// =============================================================================
// COMPOSED CAPABILITIES (What Commander Sees)
// =============================================================================

#[derive(Clone, Debug)]
struct ComposedCapability {
    id: usize,
    name: String,
    piece_ids: Vec<usize>,
    center_x: usize,
    center_y: usize,
    // Aggregated capabilities
    detect_bonus: i32,
    track_bonus: i32,
    strike_bonus: i32,
    recon_bonus: i32,
    authorize_bonus: i32,
    relay_bonus: i32,
    // Status
    total_fuel: i32,
    max_fuel: i32,
    team: Team,
}

impl ComposedCapability {
    fn symbol(&self) -> &'static str {
        // Pick symbol based on primary capability
        if self.strike_bonus >= 3 && self.authorize_bonus >= 2 {
            "⚔" // Strike package
        } else if self.detect_bonus >= 3 && self.track_bonus >= 2 {
            "◎" // ISR package
        } else if self.recon_bonus >= 2 {
            "◇" // Recon team
        } else if self.relay_bonus >= 2 {
            "◈" // Support/relay
        } else {
            "●" // Generic
        }
    }

    fn primary_capability(&self) -> &'static str {
        if self.strike_bonus >= 3 && self.authorize_bonus >= 2 {
            "STRIKE_READY"
        } else if self.detect_bonus >= 3 && self.track_bonus >= 2 {
            "ISR_PACKAGE"
        } else if self.recon_bonus >= 3 {
            "RECON_TEAM"
        } else if self.relay_bonus >= 2 {
            "SUPPORT_NET"
        } else if self.authorize_bonus >= 3 {
            "COMMAND_ELM"
        } else {
            "TASK_FORCE"
        }
    }

    fn fuel_percent(&self) -> f32 {
        if self.max_fuel == 0 {
            1.0
        } else {
            self.total_fuel as f32 / self.max_fuel as f32
        }
    }
}

// =============================================================================
// OBJECTIVES
// =============================================================================

#[derive(Clone, Debug)]
struct Objective {
    id: usize,
    name: String,
    description: String,
    x: usize,
    y: usize,
    // Required capabilities
    detect_required: i32,
    track_required: i32,
    strike_required: i32,
    authorize_required: i32,
    // Status
    completed: bool,
    assigned_capability: Option<usize>,
    turns_remaining: i32,
    points: i32,
}

impl Objective {
    fn symbol(&self) -> &'static str {
        if self.completed {
            "✓"
        } else if self.assigned_capability.is_some() {
            "→"
        } else {
            "★"
        }
    }

    fn can_be_accomplished_by(&self, cap: &ComposedCapability) -> bool {
        cap.detect_bonus >= self.detect_required
            && cap.track_bonus >= self.track_required
            && cap.strike_bonus >= self.strike_required
            && cap.authorize_bonus >= self.authorize_required
    }
}

// =============================================================================
// COURSE OF ACTION
// =============================================================================

#[derive(Clone, Debug)]
struct CourseOfAction {
    capability_id: usize,
    capability_name: String,
    objective_id: usize,
    turns_to_complete: i32,
    fuel_cost: i32,
    success_chance: i32, // Percentage
    risk_level: &'static str,
    description: String,
}

// =============================================================================
// GAME STATE
// =============================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
enum GamePhase {
    SelectObjective,
    SelectCOA,
    Executing,
    EnemyTurn,
}

struct GameState {
    width: usize,
    height: usize,
    terrain: Vec<Vec<Terrain>>,
    pieces: Vec<Piece>,
    capabilities: Vec<ComposedCapability>,
    objectives: Vec<Objective>,
    current_coas: Vec<CourseOfAction>,
    seed: u32,
    turn: u32,
    phase: GamePhase,
    selected_objective: Option<usize>,
    selected_coa: Option<usize>,
    message: String,
    score: i32,
}

impl GameState {
    fn generate(width: usize, height: usize, seed: u32) -> Self {
        let perlin = Perlin::new(seed);
        let mut terrain = vec![vec![Terrain::Plains; width]; height];
        let mut rng = rand::thread_rng();

        // Generate terrain
        for (y, row) in terrain.iter_mut().enumerate() {
            for (x, cell) in row.iter_mut().enumerate() {
                let nx = x as f64 / width as f64 * 4.0;
                let ny = y as f64 / height as f64 * 4.0;
                let elevation = perlin.get([nx, ny]);
                let detail = perlin.get([nx * 2.0 + 100.0, ny * 2.0 + 100.0]) * 0.3;
                let total = elevation + detail;

                *cell = if total < -0.4 {
                    Terrain::DeepWater
                } else if total < -0.2 {
                    Terrain::ShallowWater
                } else if total < 0.2 {
                    Terrain::Plains
                } else if total < 0.4 {
                    if rng.gen_bool(0.4) {
                        Terrain::Forest
                    } else {
                        Terrain::Plains
                    }
                } else if total < 0.6 {
                    Terrain::Hills
                } else {
                    Terrain::Mountain
                };
            }
        }

        // Add urban areas
        for _ in 0..rng.gen_range(2..4) {
            let cx = rng.gen_range(3..width - 3);
            let cy = rng.gen_range(2..height - 2);
            for dy in 0..=rng.gen_range(1..3) {
                for dx in 0..=rng.gen_range(1..3) {
                    if cx + dx < width
                        && cy + dy < height
                        && terrain[cy + dy][cx + dx] != Terrain::DeepWater
                    {
                        terrain[cy + dy][cx + dx] = Terrain::Urban;
                    }
                }
            }
        }

        // Add bases
        terrain[height / 2][2] = Terrain::Base;
        terrain[height / 2][width - 3] = Terrain::Base;

        // Generate Blue pieces (player)
        let mut pieces = Vec::new();
        let mut next_id = 0;

        let blue_types = [
            PieceType::Sensor(DetectionMode::EO),
            PieceType::Sensor(DetectionMode::IR),
            PieceType::Sensor(DetectionMode::Radar),
            PieceType::Scout,
            PieceType::Striker,
            PieceType::Striker,
            PieceType::Support,
            PieceType::Authority,
        ];

        for piece_type in blue_types {
            let (x, y) = Self::find_spawn(&terrain, 0, width / 4, &pieces, &mut rng);
            pieces.push(Piece {
                id: next_id,
                piece_type,
                team: Team::Blue,
                x,
                y,
                fuel: 10,
                max_fuel: 10,
            });
            next_id += 1;
        }

        // Generate Red pieces (enemy)
        let red_types = [
            PieceType::Sensor(DetectionMode::IR),
            PieceType::Sensor(DetectionMode::Acoustic),
            PieceType::Scout,
            PieceType::Scout,
            PieceType::Striker,
            PieceType::Striker,
            PieceType::Support,
            PieceType::Authority,
        ];

        for piece_type in red_types {
            let (x, y) = Self::find_spawn(&terrain, width * 3 / 4, width, &pieces, &mut rng);
            pieces.push(Piece {
                id: next_id,
                piece_type,
                team: Team::Red,
                x,
                y,
                fuel: 10,
                max_fuel: 10,
            });
            next_id += 1;
        }

        // Generate initial objectives
        let objectives = vec![
            Objective {
                id: 0,
                name: "TRACK HVT".to_string(),
                description: "Locate and track high-value target".to_string(),
                x: width / 2,
                y: height / 3,
                detect_required: 2,
                track_required: 2,
                strike_required: 0,
                authorize_required: 0,
                completed: false,
                assigned_capability: None,
                turns_remaining: 0,
                points: 100,
            },
            Objective {
                id: 1,
                name: "SECURE AREA".to_string(),
                description: "Establish presence and secure zone".to_string(),
                x: width / 2 + 5,
                y: height * 2 / 3,
                detect_required: 1,
                track_required: 0,
                strike_required: 2,
                authorize_required: 2,
                completed: false,
                assigned_capability: None,
                turns_remaining: 0,
                points: 150,
            },
            Objective {
                id: 2,
                name: "RECON ZONE".to_string(),
                description: "Reconnaissance of unknown area".to_string(),
                x: width / 2 - 3,
                y: height / 2,
                detect_required: 3,
                track_required: 1,
                strike_required: 0,
                authorize_required: 0,
                completed: false,
                assigned_capability: None,
                turns_remaining: 0,
                points: 75,
            },
        ];

        let mut state = GameState {
            width,
            height,
            terrain,
            pieces,
            capabilities: Vec::new(),
            objectives,
            current_coas: Vec::new(),
            seed,
            turn: 1,
            phase: GamePhase::SelectObjective,
            selected_objective: None,
            selected_coa: None,
            message: "Select an objective with [1-3], then choose a COA".to_string(),
            score: 0,
        };

        state.recompute_capabilities();
        state
    }

    fn find_spawn(
        terrain: &[Vec<Terrain>],
        min_x: usize,
        max_x: usize,
        existing: &[Piece],
        rng: &mut impl Rng,
    ) -> (usize, usize) {
        let height = terrain.len();
        loop {
            let x = rng.gen_range(min_x..max_x);
            let y = rng.gen_range(1..height - 1);
            if terrain[y][x].movement_cost().is_some()
                && !existing.iter().any(|p| p.x == x && p.y == y)
            {
                return (x, y);
            }
        }
    }

    /// Compute composed capabilities from piece positions
    fn recompute_capabilities(&mut self) {
        self.capabilities.clear();

        // Group Blue pieces by proximity (within 3 hexes of each other)
        let blue_pieces: Vec<&Piece> = self
            .pieces
            .iter()
            .filter(|p| p.team == Team::Blue)
            .collect();

        // Simple grouping: pieces within range form a capability
        let mut assigned: Vec<bool> = vec![false; blue_pieces.len()];
        let mut cap_id = 0;

        for i in 0..blue_pieces.len() {
            if assigned[i] {
                continue;
            }

            let mut group = vec![i];
            assigned[i] = true;

            // Find nearby pieces
            for j in (i + 1)..blue_pieces.len() {
                if assigned[j] {
                    continue;
                }
                let dx = (blue_pieces[i].x as i32 - blue_pieces[j].x as i32).abs();
                let dy = (blue_pieces[i].y as i32 - blue_pieces[j].y as i32).abs();
                if dx <= 3 && dy <= 3 {
                    group.push(j);
                    assigned[j] = true;
                }
            }

            // Compute aggregated capabilities
            let mut detect = 0;
            let mut track = 0;
            let mut strike = 0;
            let mut recon = 0;
            let mut authorize = 0;
            let mut relay = 0;
            let mut total_fuel = 0;
            let mut max_fuel = 0;
            let mut sum_x = 0;
            let mut sum_y = 0;

            let piece_ids: Vec<usize> = group.iter().map(|&idx| blue_pieces[idx].id).collect();

            for &idx in &group {
                let piece = blue_pieces[idx];
                sum_x += piece.x;
                sum_y += piece.y;
                total_fuel += piece.fuel;
                max_fuel += piece.max_fuel;

                match piece.piece_type {
                    PieceType::Sensor(mode) => {
                        detect += 3;
                        track += 2;
                        // Bonus for specific modes
                        if mode == DetectionMode::Radar {
                            detect += 1;
                        }
                    }
                    PieceType::Scout => {
                        recon += 3;
                        detect += 1;
                    }
                    PieceType::Striker => {
                        strike += 3;
                    }
                    PieceType::Support => {
                        relay += 3;
                    }
                    PieceType::Authority => {
                        authorize += 4;
                        strike += 1; // Can authorize strikes
                    }
                }
            }

            // Synergy bonuses
            if group.len() >= 2 {
                detect += 1; // Multi-sensor fusion
            }
            if authorize > 0 && strike > 0 {
                strike += 2; // Authorized strike bonus
            }

            let name = format!(
                "{}-{}",
                if strike >= 3 {
                    "STRIKE"
                } else if detect >= 3 {
                    "ISR"
                } else if recon >= 3 {
                    "RECON"
                } else {
                    "TASK"
                },
                cap_id + 1
            );

            self.capabilities.push(ComposedCapability {
                id: cap_id,
                name,
                piece_ids,
                center_x: sum_x / group.len(),
                center_y: sum_y / group.len(),
                detect_bonus: detect,
                track_bonus: track,
                strike_bonus: strike,
                recon_bonus: recon,
                authorize_bonus: authorize,
                relay_bonus: relay,
                total_fuel,
                max_fuel,
                team: Team::Blue,
            });

            cap_id += 1;
        }
    }

    /// Generate COAs for a selected objective
    fn generate_coas(&mut self, objective_idx: usize) {
        self.current_coas.clear();
        let objective = &self.objectives[objective_idx];

        if objective.completed {
            return;
        }

        for cap in &self.capabilities {
            if !objective.can_be_accomplished_by(cap) {
                continue;
            }

            // Calculate distance
            let dx = (cap.center_x as i32 - objective.x as i32).abs();
            let dy = (cap.center_y as i32 - objective.y as i32).abs();
            let distance = dx.max(dy);

            let turns = (distance / 3).max(1);
            let fuel_cost = distance / 2 + 1;

            // Success chance based on capability margin
            let margin = (cap.detect_bonus - objective.detect_required)
                + (cap.track_bonus - objective.track_required)
                + (cap.strike_bonus - objective.strike_required)
                + (cap.authorize_bonus - objective.authorize_required);
            let success = (60 + margin * 10).clamp(30, 95);

            // Risk based on exposure
            let risk = if cap.recon_bonus >= 2 {
                "LOW"
            } else if cap.strike_bonus >= 3 {
                "HIGH"
            } else {
                "MEDIUM"
            };

            let desc = format!(
                "{} moves to objective, executes {}",
                cap.name, objective.name
            );

            self.current_coas.push(CourseOfAction {
                capability_id: cap.id,
                capability_name: cap.name.clone(),
                objective_id: objective.id,
                turns_to_complete: turns,
                fuel_cost,
                success_chance: success,
                risk_level: risk,
                description: desc,
            });
        }

        if self.current_coas.is_empty() {
            self.message = "No capable force available for this objective!".to_string();
        }
    }

    /// Execute selected COA
    fn execute_coa(&mut self, coa_idx: usize) {
        let coa = &self.current_coas[coa_idx];

        // Mark objective as in progress
        if let Some(obj) = self
            .objectives
            .iter_mut()
            .find(|o| o.id == coa.objective_id)
        {
            obj.assigned_capability = Some(coa.capability_id);
            obj.turns_remaining = coa.turns_to_complete;
        }

        // Deduct fuel from capability's pieces
        let cap = &self.capabilities[coa.capability_id];
        let fuel_per_piece = coa.fuel_cost / cap.piece_ids.len().max(1) as i32;
        for &piece_id in &cap.piece_ids {
            if let Some(piece) = self.pieces.iter_mut().find(|p| p.id == piece_id) {
                piece.fuel = (piece.fuel - fuel_per_piece).max(0);
            }
        }

        self.message = format!(
            "Executing: {} → {}",
            coa.capability_name,
            self.objectives
                .iter()
                .find(|o| o.id == coa.objective_id)
                .map(|o| &o.name)
                .unwrap_or(&"?".to_string())
        );

        self.phase = GamePhase::Executing;
        self.selected_objective = None;
        self.selected_coa = None;
    }

    /// Process turn
    fn end_turn(&mut self) {
        let mut rng = rand::thread_rng();

        // Process objectives in progress
        for obj in &mut self.objectives {
            if obj.assigned_capability.is_some() && !obj.completed {
                obj.turns_remaining -= 1;

                if obj.turns_remaining <= 0 {
                    // Resolve objective
                    let coa = self.current_coas.iter().find(|c| c.objective_id == obj.id);

                    let success_roll = rng.gen_range(0..100);
                    let success_chance = coa.map(|c| c.success_chance).unwrap_or(50);

                    if success_roll < success_chance {
                        obj.completed = true;
                        self.score += obj.points;
                        self.message =
                            format!("SUCCESS: {} completed! +{} points", obj.name, obj.points);
                    } else {
                        obj.assigned_capability = None;
                        self.message =
                            format!("FAILED: {} - retry with different approach", obj.name);
                    }
                }
            }
        }

        // Move enemy pieces toward player (simple AI)
        let blue_center_x: i32 = self
            .pieces
            .iter()
            .filter(|p| p.team == Team::Blue)
            .map(|p| p.x as i32)
            .sum::<i32>()
            / self
                .pieces
                .iter()
                .filter(|p| p.team == Team::Blue)
                .count()
                .max(1) as i32;

        // Collect occupied positions first to avoid borrow checker issues
        let occupied: Vec<(usize, usize, usize)> =
            self.pieces.iter().map(|p| (p.id, p.x, p.y)).collect();

        for piece in &mut self.pieces {
            if piece.team == Team::Red && piece.fuel > 0 {
                let dx = if (piece.x as i32) > blue_center_x {
                    -1
                } else {
                    1
                };
                let new_x = ((piece.x as i32) + dx).max(0) as usize;

                if new_x < self.width
                    && self.terrain[piece.y][new_x].movement_cost().is_some()
                    && !occupied
                        .iter()
                        .any(|(id, x, y)| *x == new_x && *y == piece.y && *id != piece.id)
                {
                    piece.x = new_x;
                    piece.fuel -= 1;
                }
            }
        }

        // Refuel pieces at base
        for piece in &mut self.pieces {
            if self.terrain[piece.y][piece.x] == Terrain::Base {
                piece.fuel = piece.max_fuel;
            }
        }

        self.turn += 1;
        self.phase = GamePhase::SelectObjective;
        self.recompute_capabilities();

        // Generate new objective occasionally
        if self.turn % 5 == 0 && self.objectives.iter().filter(|o| !o.completed).count() < 4 {
            let x = rng.gen_range(self.width / 3..self.width * 2 / 3);
            let y = rng.gen_range(2..self.height - 2);
            let id = self.objectives.len();
            self.objectives.push(Objective {
                id,
                name: format!("TARGET-{}", id),
                description: "New objective appeared".to_string(),
                x,
                y,
                detect_required: rng.gen_range(1..4),
                track_required: rng.gen_range(0..3),
                strike_required: rng.gen_range(0..3),
                authorize_required: if rng.gen_bool(0.3) { 2 } else { 0 },
                completed: false,
                assigned_capability: None,
                turns_remaining: 0,
                points: rng.gen_range(50..200),
            });
        }
    }

    fn is_enemy_visible(&self, enemy: &Piece) -> bool {
        for cap in &self.capabilities {
            let dx = (cap.center_x as i32 - enemy.x as i32).abs();
            let dy = (cap.center_y as i32 - enemy.y as i32).abs();
            let distance = dx.max(dy);

            // Detection range based on capability's sensors
            let range = if cap.detect_bonus >= 4 {
                8
            } else if cap.detect_bonus >= 2 {
                5
            } else {
                3
            };

            if distance <= range {
                return true;
            }
        }
        false
    }

    fn render_map(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Top border
        let mut header = String::from("   ");
        for x in 0..self.width {
            header.push_str(&format!("{:>2}", x % 10));
        }
        lines.push(Line::from(header).style(Style::default().fg(Color::DarkGray)));

        for y in 0..self.height {
            let mut spans = vec![Span::styled(
                format!("{:>2} ", y),
                Style::default().fg(Color::DarkGray),
            )];

            for x in 0..self.width {
                // Check for objective
                if let Some(obj) = self
                    .objectives
                    .iter()
                    .find(|o| o.x == x && o.y == y && !o.completed)
                {
                    let style = if obj.assigned_capability.is_some() {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD)
                    };
                    spans.push(Span::styled(format!("{} ", obj.symbol()), style));
                    continue;
                }

                // Check for composed capability (Blue)
                if let Some(cap) = self
                    .capabilities
                    .iter()
                    .find(|c| c.center_x == x && c.center_y == y)
                {
                    let style = Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD);
                    spans.push(Span::styled(format!("{} ", cap.symbol()), style));
                    continue;
                }

                // Check for enemy piece (only if visible)
                if let Some(piece) = self
                    .pieces
                    .iter()
                    .find(|p| p.x == x && p.y == y && p.team == Team::Red)
                {
                    if self.is_enemy_visible(piece) {
                        spans.push(Span::styled("? ", Style::default().fg(Color::Red)));
                        continue;
                    }
                }

                // Terrain
                let terrain = &self.terrain[y][x];
                spans.push(Span::styled(
                    format!("{} ", terrain.symbol()),
                    terrain.style(),
                ));
            }

            lines.push(Line::from(spans));
        }

        lines
    }

    fn render_capabilities(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from("COMPOSED CAPABILITIES:").style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Cyan),
            ),
            Line::from(""),
        ];

        for cap in &self.capabilities {
            let fuel_pct = cap.fuel_percent();
            let fuel_color = if fuel_pct < 0.3 {
                Color::Red
            } else if fuel_pct < 0.6 {
                Color::Yellow
            } else {
                Color::Green
            };

            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} ", cap.symbol()),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(
                    "{} @ ({},{}) ",
                    cap.primary_capability(),
                    cap.center_x,
                    cap.center_y
                )),
            ]));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("DET:{} ", cap.detect_bonus),
                    Style::default().fg(Color::Green),
                ),
                Span::styled(
                    format!("TRK:{} ", cap.track_bonus),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    format!("STR:{} ", cap.strike_bonus),
                    Style::default().fg(Color::Red),
                ),
                Span::styled(
                    format!("AUTH:{}", cap.authorize_bonus),
                    Style::default().fg(Color::Magenta),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::raw("  Fuel: "),
                Span::styled(
                    "█".repeat((fuel_pct * 8.0) as usize),
                    Style::default().fg(fuel_color),
                ),
                Span::styled(
                    "░".repeat(8 - (fuel_pct * 8.0) as usize),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(format!(" ({} units)", cap.piece_ids.len())),
            ]));
            lines.push(Line::from(""));
        }

        lines
    }

    fn render_objectives(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from("OBJECTIVES:").style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Magenta),
            ),
            Line::from(""),
        ];

        for (i, obj) in self.objectives.iter().filter(|o| !o.completed).enumerate() {
            let status = if obj.assigned_capability.is_some() {
                format!("IN PROGRESS ({} turns)", obj.turns_remaining)
            } else {
                "AVAILABLE".to_string()
            };

            let num_style = if self.selected_objective == Some(obj.id) {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            lines.push(Line::from(vec![
                Span::styled(format!("[{}] ", i + 1), num_style),
                Span::styled(obj.name.clone(), Style::default().fg(Color::Magenta)),
                Span::raw(format!(" @ ({},{}) ", obj.x, obj.y)),
            ]));
            lines.push(Line::from(vec![
                Span::raw("    Requires: "),
                if obj.detect_required > 0 {
                    Span::styled(
                        format!("DET:{} ", obj.detect_required),
                        Style::default().fg(Color::Green),
                    )
                } else {
                    Span::raw("")
                },
                if obj.track_required > 0 {
                    Span::styled(
                        format!("TRK:{} ", obj.track_required),
                        Style::default().fg(Color::Yellow),
                    )
                } else {
                    Span::raw("")
                },
                if obj.strike_required > 0 {
                    Span::styled(
                        format!("STR:{} ", obj.strike_required),
                        Style::default().fg(Color::Red),
                    )
                } else {
                    Span::raw("")
                },
                if obj.authorize_required > 0 {
                    Span::styled(
                        format!("AUTH:{}", obj.authorize_required),
                        Style::default().fg(Color::Magenta),
                    )
                } else {
                    Span::raw("")
                },
            ]));
            lines.push(Line::from(format!("    {} | {} pts", status, obj.points)));
            lines.push(Line::from(""));
        }

        lines
    }

    fn render_coas(&self) -> Vec<Line<'static>> {
        if self.current_coas.is_empty() {
            return vec![Line::from("Select an objective [1-3] to see COAs")
                .style(Style::default().fg(Color::DarkGray))];
        }

        let mut lines = vec![
            Line::from("COURSES OF ACTION:").style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Yellow),
            ),
            Line::from(""),
        ];

        for (i, coa) in self.current_coas.iter().enumerate() {
            let key = (b'A' + i as u8) as char;
            let selected = self.selected_coa == Some(i);

            let key_style = if selected {
                Style::default().fg(Color::Black).bg(Color::Yellow)
            } else {
                Style::default().fg(Color::Yellow)
            };

            let risk_color = match coa.risk_level {
                "LOW" => Color::Green,
                "HIGH" => Color::Red,
                _ => Color::Yellow,
            };

            lines.push(Line::from(vec![
                Span::styled(format!("[{}] ", key), key_style),
                Span::raw(coa.capability_name.clone()),
            ]));
            lines.push(Line::from(format!(
                "    Time: {} turns  Fuel: {}  Success: {}%",
                coa.turns_to_complete, coa.fuel_cost, coa.success_chance
            )));
            lines.push(Line::from(vec![
                Span::raw("    Risk: "),
                Span::styled(coa.risk_level.to_string(), Style::default().fg(risk_color)),
            ]));
            lines.push(Line::from(""));
        }

        lines.push(
            Line::from("Press [A/B/C] to select, [ENTER] to execute")
                .style(Style::default().fg(Color::DarkGray)),
        );

        lines
    }

    fn render_status(&self) -> Vec<Line<'static>> {
        let phase_str = match self.phase {
            GamePhase::SelectObjective => "SELECT OBJECTIVE",
            GamePhase::SelectCOA => "SELECT COA",
            GamePhase::Executing => "EXECUTING...",
            GamePhase::EnemyTurn => "ENEMY TURN",
        };

        vec![
            Line::from(format!(
                "Turn: {}  Score: {}  Phase: {}",
                self.turn, self.score, phase_str
            ))
            .style(Style::default().add_modifier(Modifier::BOLD)),
            Line::from(self.message.clone()).style(Style::default().fg(Color::Yellow)),
        ]
    }
}

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let mut seed: u32 = rand::thread_rng().gen();
    let mut state = GameState::generate(45, 14, seed);

    loop {
        terminal.draw(|frame| {
            let area = frame.area();

            // Vertical: status (top) | main (middle) | COAs (bottom)
            let main_split = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),  // Status
                    Constraint::Min(16),    // Map + capabilities
                    Constraint::Length(12), // COAs
                ])
                .split(area);

            // Status bar
            let status_block = Block::default()
                .title(" HIVE Commander ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan));
            let status = Paragraph::new(state.render_status()).block(status_block);
            frame.render_widget(status, main_split[0]);

            // Middle: map (left) | info (right)
            let middle_split = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
                .split(main_split[1]);

            // Map
            let map_block = Block::default()
                .title(format!(" Tactical Map - Seed: {} ", state.seed))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green));
            let map = Paragraph::new(state.render_map()).block(map_block);
            frame.render_widget(map, middle_split[0]);

            // Info: capabilities (top) | objectives (bottom)
            let info_split = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(middle_split[1]);

            let cap_block = Block::default()
                .title(" Your Forces ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan));
            let caps = Paragraph::new(state.render_capabilities())
                .block(cap_block)
                .wrap(Wrap { trim: true });
            frame.render_widget(caps, info_split[0]);

            let obj_block = Block::default()
                .title(" Objectives ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta));
            let objs = Paragraph::new(state.render_objectives())
                .block(obj_block)
                .wrap(Wrap { trim: true });
            frame.render_widget(objs, info_split[1]);

            // COA panel (bottom)
            let coa_block = Block::default()
                .title(" Courses of Action ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow));
            let coas = Paragraph::new(state.render_coas())
                .block(coa_block)
                .wrap(Wrap { trim: true });
            frame.render_widget(coas, main_split[2]);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => break,

                        // New game
                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            seed = rand::thread_rng().gen();
                            state = GameState::generate(45, 14, seed);
                        }

                        // Select objective (1, 2, 3)
                        KeyCode::Char('1') => {
                            let available: Vec<_> = state
                                .objectives
                                .iter()
                                .filter(|o| !o.completed && o.assigned_capability.is_none())
                                .collect();
                            if !available.is_empty() {
                                state.selected_objective = Some(available[0].id);
                                state.generate_coas(available[0].id);
                                state.phase = GamePhase::SelectCOA;
                            }
                        }
                        KeyCode::Char('2') => {
                            let available: Vec<_> = state
                                .objectives
                                .iter()
                                .filter(|o| !o.completed && o.assigned_capability.is_none())
                                .collect();
                            if available.len() > 1 {
                                state.selected_objective = Some(available[1].id);
                                state.generate_coas(available[1].id);
                                state.phase = GamePhase::SelectCOA;
                            }
                        }
                        KeyCode::Char('3') => {
                            let available: Vec<_> = state
                                .objectives
                                .iter()
                                .filter(|o| !o.completed && o.assigned_capability.is_none())
                                .collect();
                            if available.len() > 2 {
                                state.selected_objective = Some(available[2].id);
                                state.generate_coas(available[2].id);
                                state.phase = GamePhase::SelectCOA;
                            }
                        }

                        // Select COA (A, B, C)
                        KeyCode::Char('a') | KeyCode::Char('A') => {
                            if !state.current_coas.is_empty() {
                                state.selected_coa = Some(0);
                            }
                        }
                        KeyCode::Char('b') | KeyCode::Char('B') => {
                            if state.current_coas.len() > 1 {
                                state.selected_coa = Some(1);
                            }
                        }
                        KeyCode::Char('c') | KeyCode::Char('C') => {
                            if state.current_coas.len() > 2 {
                                state.selected_coa = Some(2);
                            }
                        }

                        // Execute / End turn
                        KeyCode::Enter | KeyCode::Char(' ') => {
                            if let Some(coa_idx) = state.selected_coa {
                                state.execute_coa(coa_idx);
                                state.current_coas.clear();
                            }
                            state.end_turn();
                        }

                        _ => {}
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}
