use eframe::egui;
use rusqlite::{types::ValueRef, Connection, OpenFlags};
use std::collections::HashSet;
use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("hk.sahayana.ZedWorkspaceManager")
            .with_inner_size([1240.0, 760.0])
            .with_min_inner_size([960.0, 540.0])
            .with_title("Zed Workspace Manager - SQLite Inspector & Query Builder"),
        ..Default::default()
    };

    eframe::run_native(
        "Zed Workspace Manager",
        options,
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx);
            Ok(Box::new(ZedWorkspaceManager::new()))
        }),
    )
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let font_candidates = [
        "/System/Library/Fonts/AppleSDGothicNeo.ttc", // macOS
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", // Linux (Ubuntu/Debian)
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", // Linux (Arch/Fedora)
        "C:\\Windows\\Fonts\\malgun.ttf",             // Windows
    ];

    for path in font_candidates {
        if let Ok(font_data) = std::fs::read(path) {
            fonts.font_data.insert(
                "korean_font".to_owned(),
                egui::FontData::from_owned(font_data),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "korean_font".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("korean_font".to_owned());
            ctx.set_fonts(fonts);
            break;
        }
    }
}

fn escape_sql_ident(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

fn format_cell_single_line(raw: &str) -> String {
    const MAX_CELL_CHARS: usize = 120;
    let mut out = String::with_capacity(raw.len().min(MAX_CELL_CHARS * 3));
    let mut count = 0;

    for c in raw.chars() {
        if count >= MAX_CELL_CHARS {
            out.push_str("...");
            break;
        }
        if c == '\n' || c == '\r' || c == '\t' {
            out.push(' ');
        } else {
            out.push(c);
        }
        count += 1;
    }
    out
}

fn format_tooltip_safe(raw: &str) -> String {
    const MAX_TOOLTIP_CHARS: usize = 500;
    let total_chars = raw.chars().count();
    if total_chars > MAX_TOOLTIP_CHARS {
        let head: String = raw.chars().take(MAX_TOOLTIP_CHARS).collect();
        format!(
            "{}...\n\n[총 {}자 중 앞 {}자만 표시]",
            head, total_chars, MAX_TOOLTIP_CHARS
        )
    } else {
        raw.to_string()
    }
}

fn column_width_for(col_name: &str) -> f32 {
    match col_name {
        "paths" | "path" | "key" | "value" => 380.0,
        "id" | "workspace_id" | "window_id" | "user_id" | "display" | "fullscreen" => 105.0,
        _ => 155.0,
    }
}

struct ZedWorkspaceManager {
    discovered_dbs: Vec<(String, String)>, // (표시 이름, 전체 경로)
    db_path: String,
    tables: Vec<String>,
    selected_table: String,
    columns: Vec<String>,
    pk_column: String,
    rows: Vec<Vec<String>>,
    checked_rows: HashSet<usize>,
    filter_text: String,
    generated_sql: String,
    status_msg: String,
}

impl ZedWorkspaceManager {
    fn new() -> Self {
        let discovered_dbs = Self::discover_zed_dbs();
        let default_path = discovered_dbs
            .first()
            .map(|(_, path)| path.clone())
            .unwrap_or_else(|| "db.sqlite".to_string());

        let mut app = Self {
            discovered_dbs,
            db_path: default_path,
            tables: Vec::new(),
            selected_table: String::new(),
            columns: Vec::new(),
            pk_column: "id".to_string(),
            rows: Vec::new(),
            checked_rows: HashSet::new(),
            filter_text: String::new(),
            generated_sql: String::new(),
            status_msg: "준비 완료".to_string(),
        };
        app.refresh_all();
        app
    }

    fn channel_priority(channel: &str) -> u8 {
        match channel {
            "0-stable" => 0,
            "0-preview" => 1,
            "0-nightly" => 2,
            "0-dev" => 3,
            "0-global" => 9,
            _ => 5,
        }
    }

    fn discover_zed_dbs() -> Vec<(String, String)> {
        let mut found: Vec<(u8, String, String)> = Vec::new();
        let mut base_dirs: Vec<PathBuf> = Vec::new();

        if let Some(data_local) = dirs::data_local_dir() {
            base_dirs.push(data_local.join("Zed/db"));
            base_dirs.push(data_local.join("zed/db"));
        }
        if let Some(data_dir) = dirs::data_dir() {
            let p1 = data_dir.join("Zed/db");
            let p2 = data_dir.join("zed/db");
            if !base_dirs.contains(&p1) {
                base_dirs.push(p1);
            }
            if !base_dirs.contains(&p2) {
                base_dirs.push(p2);
            }
        }

        for base in base_dirs {
            if let Ok(entries) = std::fs::read_dir(&base) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let db_file = path.join("db.sqlite");
                        if db_file.exists() {
                            let dir_name = entry.file_name().to_string_lossy().to_string();
                            let prio = Self::channel_priority(&dir_name);
                            let label = format!("{}/db.sqlite", dir_name);
                            let full = db_file.to_string_lossy().to_string();
                            if !found.iter().any(|(_, _, existing)| existing == &full) {
                                found.push((prio, label, full));
                            }
                        }
                    }
                }
            }
        }

        found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        found
            .into_iter()
            .map(|(_, label, full)| (label, full))
            .collect()
    }

    fn open_readonly_db(&self) -> Result<Connection, String> {
        let path = PathBuf::from(self.db_path.trim());
        if !path.exists() {
            return Err(format!("DB 파일을 찾을 수 없습니다: {}", self.db_path));
        }
        Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| format!("DB 열기 실패: {}", e))
    }

    fn refresh_all(&mut self) {
        self.discovered_dbs = Self::discover_zed_dbs();
        let prev_table = self.selected_table.clone();

        match self.open_readonly_db() {
            Ok(conn) => {
                let mut stmt = match conn.prepare(
                    "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name;",
                ) {
                    Ok(s) => s,
                    Err(e) => {
                        self.status_msg = format!("테이블 목록 조회 실패: {}", e);
                        return;
                    }
                };

                if let Ok(iter) = stmt.query_map([], |row| row.get::<_, String>(0)) {
                    self.tables = iter.filter_map(Result::ok).collect();
                }

                if let Some(first_table) = self.tables.first().cloned() {
                    if self.tables.contains(&prev_table) {
                        self.selected_table = prev_table;
                    } else if self.tables.iter().any(|t| t == "workspaces") {
                        self.selected_table = "workspaces".to_string();
                    } else {
                        self.selected_table = first_table;
                    }
                    self.load_table_data();
                } else {
                    self.columns.clear();
                    self.rows.clear();
                    self.status_msg = "테이블이 없습니다.".to_string();
                }
            }
            Err(err) => {
                self.status_msg = err;
            }
        }
    }

    fn load_table_data(&mut self) {
        if self.selected_table.is_empty() {
            return;
        }
        self.checked_rows.clear();

        let conn = match self.open_readonly_db() {
            Ok(c) => c,
            Err(err) => {
                self.status_msg = err;
                return;
            }
        };

        let table_ident = escape_sql_ident(&self.selected_table);
        let pragma_sql = format!("PRAGMA table_info({});", table_ident);
        let mut cols = Vec::new();
        let mut pk = None;

        if let Ok(mut stmt) = conn.prepare(&pragma_sql) {
            if let Ok(iter) = stmt.query_map([], |row| {
                let name: String = row.get(1)?;
                let is_pk: i32 = row.get(5)?;
                Ok((name, is_pk))
            }) {
                for item in iter.flatten() {
                    if item.1 > 0 && pk.is_none() {
                        pk = Some(item.0.clone());
                    }
                    cols.push(item.0);
                }
            }
        }

        self.columns = cols;
        self.pk_column = pk.unwrap_or_else(|| {
            self.columns
                .first()
                .cloned()
                .unwrap_or_else(|| "rowid".to_string())
        });

        let order_clause = if self.columns.contains(&self.pk_column) {
            format!("ORDER BY {} DESC", escape_sql_ident(&self.pk_column))
        } else {
            String::new()
        };
        let query = format!("SELECT * FROM {} {} LIMIT 2000;", table_ident, order_clause);

        let mut loaded_rows = Vec::new();
        if let Ok(mut stmt) = conn.prepare(&query) {
            let col_count = self.columns.len();
            if let Ok(mut rows) = stmt.query([]) {
                while let Ok(Some(row)) = rows.next() {
                    let mut row_vals = Vec::with_capacity(col_count);
                    for i in 0..col_count {
                        let val_str = match row.get_ref(i) {
                            Ok(ValueRef::Null) => "NULL".to_string(),
                            Ok(ValueRef::Integer(n)) => n.to_string(),
                            Ok(ValueRef::Real(f)) => f.to_string(),
                            Ok(ValueRef::Text(t)) => String::from_utf8_lossy(t).to_string(),
                            Ok(ValueRef::Blob(b)) => {
                                if !b.contains(&0) {
                                    if let Ok(s) = std::str::from_utf8(b) {
                                        s.to_string()
                                    } else {
                                        format!("<BLOB {} bytes>", b.len())
                                    }
                                } else {
                                    format!("<BLOB {} bytes>", b.len())
                                }
                            }
                            Err(_) => String::new(),
                        };
                        row_vals.push(val_str);
                    }
                    loaded_rows.push(row_vals);
                }
            }
        }

        self.rows = loaded_rows;
        self.status_msg = format!(
            "[{}] {}건 로드됨 (기준 PK: {})",
            self.selected_table,
            self.rows.len(),
            self.pk_column
        );
    }

    fn visible_row_indices(&self) -> Vec<usize> {
        let keyword = self.filter_text.trim().to_lowercase();
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                keyword.is_empty()
                    || row
                        .iter()
                        .any(|cell| cell.to_lowercase().contains(&keyword))
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    fn get_selected_pk_values(&self) -> Vec<String> {
        let Some(pk_idx) = self.columns.iter().position(|c| c == &self.pk_column) else {
            return Vec::new();
        };

        let mut indices: Vec<usize> = self.checked_rows.iter().copied().collect();
        indices.sort_unstable();

        indices
            .into_iter()
            .filter_map(|row_idx| self.rows.get(row_idx))
            .filter_map(|row| row.get(pk_idx))
            .map(|val| {
                if val == "NULL" {
                    "NULL".to_string()
                } else if val.parse::<i64>().is_ok() {
                    val.clone()
                } else {
                    format!("'{}'", val.replace('\'', "''"))
                }
            })
            .collect()
    }

    fn generate_all_select_sql(&mut self) {
        if self.selected_table.is_empty() {
            return;
        }
        self.generated_sql = format!(
            "-- [{}] 전체 데이터 조회\nSELECT *\nFROM {}\nORDER BY {} DESC;",
            self.selected_table, self.selected_table, self.pk_column
        );
    }

    fn generate_selected_select_sql(&mut self) {
        let pks = self.get_selected_pk_values();
        if pks.is_empty() {
            self.status_msg = "⚠️ 조회할 항목을 먼저 선택해 주세요.".to_string();
            return;
        }
        self.generated_sql = format!(
            "-- [{}] 선택된 {}건 조회\nSELECT *\nFROM {}\nWHERE {} IN ({});",
            self.selected_table,
            pks.len(),
            self.selected_table,
            self.pk_column,
            pks.join(", ")
        );
    }

    fn generate_delete_sql(&mut self) {
        let pks = self.get_selected_pk_values();
        if pks.is_empty() {
            self.status_msg = "⚠️ 삭제할 항목을 먼저 선택해 주세요.".to_string();
            return;
        }
        let in_clause = pks.join(", ");
        self.generated_sql = format!(
            "-- ⚠️ 주의: 실행 전 아래 SELECT 문으로 대상 데이터를 먼저 확인하세요.\n\
             -- SELECT * FROM {} WHERE {} IN ({});\n\n\
             DELETE FROM {}\n\
             WHERE {} IN ({});",
            self.selected_table,
            self.pk_column,
            in_clause,
            self.selected_table,
            self.pk_column,
            in_clause
        );
    }
}

impl eframe::App for ZedWorkspaceManager {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let visible_indices = self.visible_row_indices();

        // 1. 상단 DB 선택, 경로, 새로고침 및 테이블 선택 바
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if !self.discovered_dbs.is_empty() {
                    ui.label("DB 선택:");
                    let current_label = self
                        .discovered_dbs
                        .iter()
                        .find(|(_, p)| p == &self.db_path)
                        .map(|(l, _)| l.clone())
                        .unwrap_or_else(|| "직접 입력".to_string());

                    let mut selected_db_path = self.db_path.clone();
                    egui::ComboBox::from_id_salt("db_file_selector")
                        .selected_text(&current_label)
                        .width(165.0)
                        .show_ui(ui, |ui| {
                            for (label, full_path) in &self.discovered_dbs {
                                ui.selectable_value(
                                    &mut selected_db_path,
                                    full_path.clone(),
                                    label,
                                )
                                .on_hover_text(full_path);
                            }
                        });

                    if selected_db_path != self.db_path {
                        self.db_path = selected_db_path;
                        self.refresh_all();
                    }
                    ui.separator();
                }

                ui.label("경로:");

                // 오른쪽 위젯들을 역순으로 먼저 배치한 뒤 남은 폭 전체를 경로 입력칸에 할당
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let prev_selected = self.selected_table.clone();
                    egui::ComboBox::from_id_salt(("table_selector", &self.db_path))
                        .selected_text(&self.selected_table)
                        .width(180.0)
                        .height(180.0)
                        .show_ui(ui, |ui| {
                            for t in &self.tables {
                                ui.selectable_value(&mut self.selected_table, t.clone(), t);
                            }
                        });

                    if self.selected_table != prev_selected {
                        self.load_table_data();
                    }

                    ui.label("테이블:");
                    ui.separator();

                    if ui.button("🔄 새로고침").clicked() {
                        self.refresh_all();
                    }

                    let path_width = ui.available_width().max(120.0);
                    ui.add(egui::TextEdit::singleline(&mut self.db_path).desired_width(path_width));
                });
            });

            ui.separator();

            // 2. 선택 제어, 검색 필터 및 SQL 생성 버튼 바
            ui.horizontal(|ui| {
                if ui.button("☑ 전체 선택").clicked() {
                    for &idx in &visible_indices {
                        self.checked_rows.insert(idx);
                    }
                }
                if ui.button("🔀 선택 반전").clicked() {
                    for &idx in &visible_indices {
                        if !self.checked_rows.remove(&idx) {
                            self.checked_rows.insert(idx);
                        }
                    }
                }
                if ui.button("☐ 선택 해제").clicked() {
                    self.checked_rows.clear();
                }

                ui.label(
                    egui::RichText::new(format!(
                        "선택: {}건 (표시: {}/{}건)",
                        self.checked_rows.len(),
                        visible_indices.len(),
                        self.rows.len()
                    ))
                    .strong()
                    .color(egui::Color32::from_rgb(80, 170, 255)),
                );

                ui.separator();
                ui.label("🔍 필터:");

                // 오른쪽 SQL 버튼들과 X 버튼을 먼저 배치한 뒤 남은 폭 전체를 필터 입력칸에 할당
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("🗑 선택 삭제 SQL").clicked() {
                        self.generate_delete_sql();
                    }
                    if ui.button("🔍 선택 조회 SQL").clicked() {
                        self.generate_selected_select_sql();
                    }
                    if ui.button("📋 전체 조회 SQL").clicked() {
                        self.generate_all_select_sql();
                    }

                    if !self.filter_text.is_empty() && ui.button("✖").clicked() {
                        self.filter_text.clear();
                    }

                    let filter_width = ui.available_width().max(100.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter_text)
                            .hint_text("경로/키워드 검색...")
                            .desired_width(filter_width),
                    );
                });
            });
            ui.add_space(4.0);
        });

        // 3. 하단 SQL 출력 및 복사 패널
        egui::TopBottomPanel::bottom("bottom_sql_panel")
            .resizable(true)
            .min_height(165.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("생성된 SQL 쿼리:").strong());
                    if ui.button("📄 SQL 클립보드 복사").clicked() && !self.generated_sql.is_empty()
                    {
                        ui.output_mut(|o| o.copied_text = self.generated_sql.clone());
                        self.status_msg = "✅ SQL 쿼리가 클립보드에 복사되었습니다.".to_string();
                    }
                    if ui.button("🧹 지우기").clicked() {
                        self.generated_sql.clear();
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(&self.status_msg);
                    });
                });

                ui.add_space(4.0);

                egui::ScrollArea::vertical()
                    .id_salt("sql_scroll")
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.generated_sql)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .desired_rows(6),
                        );
                    });
                ui.add_space(4.0);
            });

        // 4. 중앙 가상 스크롤 데이터 테이블 (헤더 고정 + 가로/세로 스크롤)
        egui::CentralPanel::default().show(ctx, |ui| {
            let row_height = 24.0;
            let check_col_width = 55.0;
            let col_widths: Vec<f32> = self.columns.iter().map(|c| column_width_for(c)).collect();

            egui::ScrollArea::horizontal()
                .id_salt("table_h_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // 고정 헤더 행
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(check_col_width, row_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                let mut all_visible_checked = !visible_indices.is_empty()
                                    && visible_indices
                                        .iter()
                                        .all(|idx| self.checked_rows.contains(idx));

                                if ui.checkbox(&mut all_visible_checked, "선택").changed() {
                                    if all_visible_checked {
                                        for &idx in &visible_indices {
                                            self.checked_rows.insert(idx);
                                        }
                                    } else {
                                        for &idx in &visible_indices {
                                            self.checked_rows.remove(&idx);
                                        }
                                    }
                                }
                            },
                        );

                        for (col, &width) in self.columns.iter().zip(col_widths.iter()) {
                            ui.allocate_ui_with_layout(
                                egui::vec2(width, row_height),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.add(
                                        egui::Label::new(egui::RichText::new(col).strong())
                                            .truncate(),
                                    );
                                },
                            );
                        }
                    });

                    ui.separator();

                    if visible_indices.is_empty() {
                        ui.add_space(16.0);
                        ui.label("표시할 데이터가 없습니다.");
                        return;
                    }

                    // 가상화된 데이터 행 영역 (egui show_rows의 범위 초과 인덱스를 안전하게 필터링)
                    egui::ScrollArea::vertical()
                        .id_salt("table_v_scroll")
                        .auto_shrink([false, false])
                        .show_rows(ui, row_height, visible_indices.len(), |ui, row_range| {
                            for vis_pos in row_range {
                                let Some(&row_idx) = visible_indices.get(vis_pos) else {
                                    continue;
                                };
                                let Some(row) = self.rows.get(row_idx) else {
                                    continue;
                                };

                                ui.horizontal(|ui| {
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(check_col_width, row_height),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            let mut is_checked =
                                                self.checked_rows.contains(&row_idx);
                                            if ui.checkbox(&mut is_checked, "").changed() {
                                                if is_checked {
                                                    self.checked_rows.insert(row_idx);
                                                } else {
                                                    self.checked_rows.remove(&row_idx);
                                                }
                                            }
                                        },
                                    );

                                    for (val, &width) in row.iter().zip(col_widths.iter()) {
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(width, row_height),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                let cell_text = format_cell_single_line(val);
                                                let resp =
                                                    ui.add(egui::Label::new(cell_text).truncate());
                                                if val != "NULL" && !val.is_empty() {
                                                    resp.on_hover_text(format_tooltip_safe(val));
                                                }
                                            },
                                        );
                                    }
                                });
                            }
                        });
                });
        });
    }
}
