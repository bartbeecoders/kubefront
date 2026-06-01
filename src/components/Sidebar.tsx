import { NAV, type ViewKey } from "../views";

interface Props {
  active: ViewKey;
  onSelect: (v: ViewKey) => void;
}

export function Sidebar({ active, onSelect }: Props) {
  return (
    <nav className="sidebar">
      {NAV.map((section) => (
        <div className="nav-section" key={section.heading}>
          <div className="nav-heading">{section.heading}</div>
          {section.items.map((item) => (
            <button
              key={item.key}
              className={`nav-item${active === item.key ? " active" : ""}`}
              onClick={() => onSelect(item.key)}
            >
              <span className="nav-ico">{item.icon}</span>
              {item.label}
            </button>
          ))}
        </div>
      ))}
    </nav>
  );
}
