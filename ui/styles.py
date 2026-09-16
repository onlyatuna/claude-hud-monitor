"""
HUD UI Stylesheet (NVIDIA Alt+R / RivaTuner Aesthetic)
Sleek dark translucent background, crisp typography, dynamic accent colors, and segmented tabs.
"""

def get_progress_color(percent: float) -> str:
    if percent >= 90:
        return "#ef4444"  # Neon Red
    elif percent >= 75:
        return "#f59e0b"  # Amber Warning
    elif percent >= 50:
        return "#3b82f6"  # Tech Blue
    else:
        return "#10b981"  # GeForce Emerald Green

def get_hud_stylesheet() -> str:
    return """
    QWidget#CentralWidget {
        background-color: rgba(14, 17, 23, 0.94);
        border: 1px solid rgba(255, 255, 255, 0.14);
        border-radius: 9px;
    }
    
    QLabel {
        color: #e2e8f0;
        font-family: 'Segoe UI', 'SF Pro Display', 'Microsoft JhengHei', sans-serif;
    }
    
    QLabel#HeaderTitle {
        font-size: 10.5px;
        font-weight: 800;
        letter-spacing: 1.0px;
        color: #94a3b8;
    }
    
    QLabel#HeaderStatus {
        font-size: 9.5px;
        color: #64748b;
        font-family: 'Consolas', monospace;
    }
    
    QPushButton.TabBtn {
        background-color: rgba(255, 255, 255, 0.05);
        border: 1px solid rgba(255, 255, 255, 0.1);
        border-radius: 4px;
        color: #94a3b8;
        font-size: 9.5px;
        font-weight: 600;
        padding: 1px 6px;
        min-height: 18px;
        max-height: 18px;
    }
    
    QPushButton.TabBtn:hover {
        background-color: rgba(255, 255, 255, 0.12);
        color: #f1f5f9;
    }
    
    QPushButton.TabBtn[active="true"] {
        background-color: rgba(56, 189, 248, 0.22);
        border: 1px solid #38bdf8;
        color: #38bdf8;
        font-weight: 700;
    }
    
    QLabel#MetricTitle {
        font-size: 10px;
        font-weight: 700;
        color: #94a3b8;
        letter-spacing: 0.6px;
    }
    
    QLabel#MetricValue {
        font-size: 16px;
        font-weight: 800;
        font-family: 'Consolas', 'Courier New', monospace;
    }
    
    QLabel#SubDetail {
        font-size: 9.5px;
        color: #64748b;
    }
    
    QProgressBar {
        background-color: rgba(255, 255, 255, 0.08);
        border: none;
        border-radius: 3px;
        text-align: right;
        min-height: 5px;
        max-height: 5px;
    }
    
    QProgressBar::chunk {
        border-radius: 3px;
    }
    
    QLabel#Badge {
        background-color: rgba(255, 255, 255, 0.06);
        border: 1px solid rgba(255, 255, 255, 0.08);
        border-radius: 3px;
        padding: 1px 5px;
        font-size: 9.5px;
        color: #cbd5e1;
        font-family: 'Consolas', monospace;
    }
    
    QPushButton#LayoutToggleBtn {
        background-color: transparent;
        border: 1px solid rgba(255, 255, 255, 0.12);
        border-radius: 4px;
        color: #94a3b8;
        font-size: 11px;
        padding: 1px 4px;
        min-width: 18px;
        max-height: 18px;
    }
    
    QPushButton#LayoutToggleBtn:hover {
        background-color: rgba(255, 255, 255, 0.12);
        color: #38bdf8;
        border-color: #38bdf8;
    }
    
    QFrame#Divider {
        background-color: rgba(255, 255, 255, 0.12);
        max-width: 1px;
        min-width: 1px;
    }
    
    QMenu {
        background-color: #161920;
        border: 1px solid rgba(255, 255, 255, 0.18);
        border-radius: 6px;
        padding: 4px 0px;
    }
    
    QMenu::item {
        color: #e2e8f0;
        padding: 6px 24px 6px 20px;
        font-size: 11px;
    }
    
    QMenu::item:selected {
        background-color: #272f3d;
        color: #38bdf8;
    }
    
    QMenu::separator {
        height: 1px;
        background-color: rgba(255, 255, 255, 0.12);
        margin: 4px 8px;
    }
    """
