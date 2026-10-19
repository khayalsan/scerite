import { Search } from "lucide-react";

interface SearchFieldProps {
  id: string;
  label: string;
  placeholder: string;
  value: string;
  onChange: (value: string) => void;
}

export function SearchField({ id, label, placeholder, value, onChange }: SearchFieldProps) {
  return <div className="section-search">
    <label htmlFor={id}>{label}</label>
    <div>
      <Search size={15} aria-hidden="true" />
      <input id={id} type="search" value={value} onChange={(event) => onChange(event.currentTarget.value)} placeholder={placeholder} />
    </div>
  </div>;
}
