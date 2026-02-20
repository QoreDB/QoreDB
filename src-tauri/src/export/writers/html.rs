// SPDX-License-Identifier: Apache-2.0

use base64::{engine::general_purpose::STANDARD, Engine as _};
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufWriter};

use crate::engine::types::{ColumnInfo, Row, Value};
use crate::export::writers::ExportWriter;

pub struct HtmlWriter {
    writer: BufWriter<File>,
    bytes_written: u64,
    header_written: bool,
    rows_written: u64,
}

impl HtmlWriter {
    pub fn new(writer: BufWriter<File>) -> Self {
        Self {
            writer,
            bytes_written: 0,
            header_written: false,
            rows_written: 0,
        }
    }

    async fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.writer
            .write_all(bytes)
            .await
            .map_err(|e| e.to_string())?;
        self.bytes_written += bytes.len() as u64;
        Ok(())
    }

    fn value_to_json(value: &Value) -> serde_json::Value {
        match value {
            Value::Null => serde_json::Value::Null,
            Value::Bool(b) => serde_json::Value::Bool(*b),
            Value::Int(i) => serde_json::Value::Number((*i).into()),
            Value::Float(f) => serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .unwrap_or_else(|| serde_json::Value::String(f.to_string())),
            Value::Text(s) => serde_json::Value::String(s.clone()),
            Value::Bytes(b) => serde_json::Value::String(STANDARD.encode(b)),
            Value::Json(j) => j.clone(),
            Value::Array(arr) => {
                serde_json::Value::Array(arr.iter().map(Self::value_to_json).collect())
            }
        }
    }
}

#[async_trait::async_trait]
impl ExportWriter for HtmlWriter {
    async fn write_header(&mut self, columns: &[ColumnInfo]) -> Result<(), String> {
        if self.header_written {
            return Ok(());
        }

        let col_names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
        let col_types: Vec<&str> = columns.iter().map(|c| c.data_type.as_str()).collect();
        let cols_json = serde_json::to_string(&col_names).map_err(|e| e.to_string())?;
        let types_json = serde_json::to_string(&col_types).map_err(|e| e.to_string())?;

        let header = format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>QoreDB Export</title>
<style>
*,*::before,*::after{{box-sizing:border-box}}
:root{{--bg:#0a0a0b;--bg2:#141417;--bg3:#1c1c21;--border:#2a2a30;--fg:#e4e4e7;--fg2:#a1a1aa;--accent:#6d5dfc;--accent-hover:#7c6dff}}
body{{margin:0;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;background:var(--bg);color:var(--fg);font-size:13px;line-height:1.5}}
.toolbar{{display:flex;align-items:center;gap:8px;padding:12px 16px;background:var(--bg2);border-bottom:1px solid var(--border);flex-wrap:wrap}}
.toolbar input[type="text"]{{padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg);color:var(--fg);font-size:13px;min-width:200px}}
.toolbar input:focus{{outline:none;border-color:var(--accent)}}
.toolbar select{{padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg);color:var(--fg);font-size:13px;cursor:pointer}}
.stats{{padding:8px 16px;font-size:12px;color:var(--fg2);background:var(--bg2);border-bottom:1px solid var(--border);display:flex;gap:16px;align-items:center}}
.badge{{background:var(--bg3);padding:2px 8px;border-radius:4px;font-weight:500}}
.table-wrap{{overflow:auto;max-height:calc(100vh - 100px)}}
table{{width:100%;border-collapse:collapse;font-size:13px}}
thead{{position:sticky;top:0;z-index:1}}
th{{background:var(--bg2);padding:8px 12px;text-align:left;border-bottom:2px solid var(--border);white-space:nowrap;cursor:pointer;user-select:none;font-weight:600}}
th:hover{{background:var(--bg3)}}
th .sort{{color:var(--accent);margin-left:4px;font-size:11px}}
th .type{{font-weight:400;color:var(--fg2);font-size:11px;margin-left:4px}}
td{{padding:6px 12px;border-bottom:1px solid var(--border);white-space:nowrap;max-width:400px;overflow:hidden;text-overflow:ellipsis}}
tr:hover td{{background:var(--bg3)}}
.null{{color:var(--fg2);font-style:italic}}
.num{{color:#6ee7b7}}
.bool{{color:#fbbf24}}
.pager{{display:flex;align-items:center;gap:8px;padding:10px 16px;background:var(--bg2);border-top:1px solid var(--border);justify-content:center;position:sticky;bottom:0}}
.pager button{{padding:4px 12px;border:1px solid var(--border);border-radius:6px;background:var(--bg);color:var(--fg);cursor:pointer;font-size:12px}}
.pager button:hover:not(:disabled){{background:var(--bg3);border-color:var(--accent)}}
.pager button:disabled{{opacity:.4;cursor:default}}
.pager span{{font-size:12px;color:var(--fg2)}}
.branding{{text-align:center;padding:8px;font-size:11px;color:var(--fg2);opacity:.6}}
</style>
</head>
<body>
<div class="toolbar">
<input type="text" id="search" placeholder="Filter rows..." oninput="applyFilters()">
<select id="pageSize" onchange="applyFilters()"><option value="50">50 rows</option><option value="100" selected>100 rows</option><option value="500">500 rows</option><option value="0">All</option></select>
</div>
<div class="stats"><span>Total: <span class="badge" id="totalCount">0</span></span><span>Showing: <span class="badge" id="filteredCount">0</span></span></div>
<div class="table-wrap"><table><thead id="thead"></thead><tbody id="tbody"></tbody></table></div>
<div class="pager"><button onclick="prevPage()" id="btnPrev" disabled>&laquo; Prev</button><span id="pageInfo">-</span><button onclick="nextPage()" id="btnNext" disabled>Next &raquo;</button></div>
<div class="branding">Exported with QoreDB</div>
<script>
var COLUMNS={cols_json};
var TYPES={types_json};
var DATA=[
"#
        );

        self.write_bytes(header.as_bytes()).await?;
        self.header_written = true;
        Ok(())
    }

    async fn write_row(&mut self, columns: &[ColumnInfo], row: &Row) -> Result<(), String> {
        let mut arr = Vec::with_capacity(columns.len());
        for (idx, _col) in columns.iter().enumerate() {
            let value = row.values.get(idx).unwrap_or(&Value::Null);
            arr.push(Self::value_to_json(value));
        }

        let serialized = serde_json::to_string(&arr).map_err(|e| e.to_string())?;

        if self.rows_written > 0 {
            self.write_bytes(b",\n").await?;
        }
        self.write_bytes(serialized.as_bytes()).await?;
        self.rows_written += 1;
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), String> {
        self.writer.flush().await.map_err(|e| e.to_string())
    }

    async fn finish(&mut self) -> Result<(), String> {
        if !self.header_written {
            // No data at all — write a minimal empty page
            self.write_bytes(b"<!DOCTYPE html><html><body><p>No data.</p></body></html>")
                .await?;
            return self.flush().await;
        }

        let footer = r#"
];
var page=0,filtered=DATA,sortCol=-1,sortAsc=true;
function el(id){return document.getElementById(id)}
function esc(s){if(s===null||s===undefined)return'<span class="null">NULL</span>';s=String(s);return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;')}
function cellHtml(v,i){if(v===null)return'<span class="null">NULL</span>';var t=TYPES[i]||'';if(typeof v==='boolean')return'<span class="bool">'+v+'</span>';if(typeof v==='number')return'<span class="num">'+v+'</span>';if(typeof v==='object')return esc(JSON.stringify(v));return esc(v)}
function renderHead(){var h='<tr>';for(var i=0;i<COLUMNS.length;i++){var arrow=sortCol===i?(sortAsc?' <span class="sort">&#9650;</span>':' <span class="sort">&#9660;</span>'):'';h+='<th onclick="sortBy('+i+')">'+esc(COLUMNS[i])+'<span class="type">'+esc(TYPES[i])+'</span>'+arrow+'</th>'}h+='</tr>';el('thead').innerHTML=h}
function render(){var ps=parseInt(el('pageSize').value)||0;var start=ps?page*ps:0;var end=ps?start+ps:filtered.length;var rows=filtered.slice(start,end);var h='';for(var r=0;r<rows.length;r++){h+='<tr>';for(var c=0;c<COLUMNS.length;c++)h+='<td>'+cellHtml(rows[r][c],c)+'</td>';h+='</tr>'}el('tbody').innerHTML=h||'<tr><td colspan="'+COLUMNS.length+'" style="text-align:center;padding:24px;color:var(--fg2)">No matching rows</td></tr>';el('totalCount').textContent=DATA.length;el('filteredCount').textContent=filtered.length;var pages=ps?Math.ceil(filtered.length/ps):1;el('pageInfo').textContent=ps?(page+1)+' / '+pages:'All';el('btnPrev').disabled=page<=0;el('btnNext').disabled=ps===0||page>=pages-1}
function applyFilters(){var q=el('search').value.toLowerCase();page=0;if(!q){filtered=DATA}else{filtered=DATA.filter(function(row){for(var i=0;i<row.length;i++){var v=row[i];if(v!==null&&String(v).toLowerCase().indexOf(q)>=0)return true}return false})}render();renderHead()}
function sortBy(col){if(sortCol===col)sortAsc=!sortAsc;else{sortCol=col;sortAsc=true}filtered.sort(function(a,b){var va=a[col],vb=b[col];if(va===null&&vb===null)return 0;if(va===null)return sortAsc?1:-1;if(vb===null)return sortAsc?-1:1;if(typeof va==='number'&&typeof vb==='number')return sortAsc?va-vb:vb-va;return sortAsc?String(va).localeCompare(String(vb)):String(vb).localeCompare(String(va))});page=0;render();renderHead()}
function prevPage(){if(page>0){page--;render()}}
function nextPage(){var ps=parseInt(el('pageSize').value)||0;if(ps&&page<Math.ceil(filtered.length/ps)-1){page++;render()}}
renderHead();applyFilters();
</script>
</body>
</html>"#;

        self.write_bytes(footer.as_bytes()).await?;
        self.flush().await
    }

    fn bytes_written(&self) -> u64 {
        self.bytes_written
    }
}
