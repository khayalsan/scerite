import { Combobox as ComboboxPrimitive } from "@base-ui/react/combobox";
import { Check, ChevronsUpDown } from "lucide-react";

interface ComboboxProps {
  id: string;
  items: string[];
  value: string;
  onValueChange: (value: string) => void;
  placeholder: string;
  disabled?: boolean;
}

export function Combobox({ id, items, value, onValueChange, placeholder, disabled = false }: ComboboxProps) {
  return <ComboboxPrimitive.Root items={items} value={value || null} onValueChange={(next) => onValueChange(next ?? "")} disabled={disabled} openOnInputClick autoHighlight>
    <ComboboxPrimitive.InputGroup className="app-combobox-input-group">
      <ComboboxPrimitive.Input id={id} className="app-combobox-input" placeholder={placeholder} />
      <ComboboxPrimitive.Trigger className="app-combobox-trigger" aria-label="Show versions"><ChevronsUpDown size={15} aria-hidden="true" /></ComboboxPrimitive.Trigger>
    </ComboboxPrimitive.InputGroup>
    <ComboboxPrimitive.Portal>
      <ComboboxPrimitive.Positioner className="app-combobox-positioner" sideOffset={4}>
        <ComboboxPrimitive.Popup className="app-combobox-popup">
          <ComboboxPrimitive.Empty className="app-combobox-empty">No matching versions.</ComboboxPrimitive.Empty>
          <ComboboxPrimitive.List className="app-combobox-list">
            {(item: string) => <ComboboxPrimitive.Item key={item} value={item} className="app-combobox-item">
              <ComboboxPrimitive.ItemIndicator className="app-combobox-item-indicator"><Check size={14} aria-hidden="true" /></ComboboxPrimitive.ItemIndicator>
              {item}
            </ComboboxPrimitive.Item>}
          </ComboboxPrimitive.List>
        </ComboboxPrimitive.Popup>
      </ComboboxPrimitive.Positioner>
    </ComboboxPrimitive.Portal>
  </ComboboxPrimitive.Root>;
}
