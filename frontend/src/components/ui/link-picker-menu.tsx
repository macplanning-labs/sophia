"use client";

import { useEffect, useRef, useState } from "react";
import { MoreVertical } from "lucide-react";
import { Button } from "@/components/ui/button";

export interface LinkPickerOption {
  value: string;
  label: string;
  group?: string;
  disabled?: boolean;
}

interface LinkPickerMenuProps {
  label: string;
  options: LinkPickerOption[];
  onSelect: (value: string) => void;
  emptyText?: string;
  /** 指定すると、縦三点のアイコンではなく、文字つきのボタンにする(何をするボタンか分かるように) */
  triggerText?: string;
}

export function LinkPickerMenu({ label, options, onSelect, emptyText = "該当する項目がありません", triggerText }: LinkPickerMenuProps) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [highlightedIndex, setHighlightedIndex] = useState(0);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // Filter and group options
  const filtered = options.filter((opt) => opt.label.toLowerCase().includes(search.toLowerCase()));

  // Group by group field
  const grouped = filtered.reduce(
    (acc, opt) => {
      const groupKey = opt.group || "_default";
      if (!acc[groupKey]) acc[groupKey] = [];
      acc[groupKey].push(opt);
      return acc;
    },
    {} as Record<string, LinkPickerOption[]>
  );

  const groupOrder = Object.keys(grouped).sort((a, b) => (a === "_default" ? 1 : b === "_default" ? -1 : 0));

  // Flatten list for keyboard navigation
  const flatList: (LinkPickerOption | { type: "header"; label: string })[] = [];
  groupOrder.forEach((groupKey) => {
    if (groupKey !== "_default") {
      flatList.push({ type: "header" as const, label: groupKey });
    }
    flatList.push(...(grouped[groupKey] || []));
  });

  const selectableItems = flatList.filter((item): item is LinkPickerOption => "value" in item && !item.disabled);

  // Focus management
  useEffect(() => {
    if (open) {
      const timer = setTimeout(() => searchInputRef.current?.focus(), 0);
      return () => clearTimeout(timer);
    }
  }, [open]);

  // Close on outside click
  useEffect(() => {
    if (!open) return;

    const handleOutsideClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };

    document.addEventListener("click", handleOutsideClick);
    return () => document.removeEventListener("click", handleOutsideClick);
  }, [open]);

  // Handle keyboard navigation
  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") {
      setOpen(false);
      return;
    }

    if (e.key === "ArrowDown") {
      e.preventDefault();
      setHighlightedIndex((prev) => (prev + 1) % selectableItems.length);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setHighlightedIndex((prev) => (prev - 1 + selectableItems.length) % selectableItems.length);
    } else if (e.key === "Enter") {
      e.preventDefault();
      const selected = selectableItems[highlightedIndex];
      if (selected) {
        onSelect(selected.value);
        setOpen(false);
      }
    }
  };

  const handleSelect = (value: string) => {
    onSelect(value);
    setOpen(false);
  };

  return (
    <div className="relative inline-block" ref={menuRef}>
      {triggerText ? (
        <Button size="sm" variant="outline" aria-label={label} title={label} onClick={() => setOpen(!open)}>
          {triggerText}
        </Button>
      ) : (
        <Button size="icon-sm" variant="ghost" aria-label={label} title={label} onClick={() => setOpen(!open)}>
          <MoreVertical className="w-4 h-4" />
        </Button>
      )}

      {open && (
        <div
          className="absolute right-0 top-full mt-1 w-64 bg-card border border-border rounded-lg shadow-lg z-50 animate-in fade-in zoom-in-95 duration-200"
          role="listbox"
        >
          {/* Search input */}
          <div className="px-3 py-2 border-b border-border">
            <input
              ref={searchInputRef}
              type="text"
              placeholder="検索..."
              value={search}
              onChange={(e) => {
                setSearch(e.target.value);
                setHighlightedIndex(0);
              }}
              onKeyDown={handleKeyDown}
              className="w-full px-2 py-1.5 text-sm bg-background border border-border rounded-md text-foreground placeholder-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
          </div>

          {/* List */}
          <div ref={listRef} className="max-h-64 overflow-y-auto">
            {flatList.length === 0 ? (
              <div className="px-3 py-4 text-sm text-muted-foreground text-center">{emptyText}</div>
            ) : (
              flatList.map((item) => {
                if ("type" in item && item.type === "header") {
                  return (
                    <div key={`header-${item.label}`} className="px-3 py-2 text-xs font-semibold text-muted-foreground bg-muted/30">
                      {item.label}
                    </div>
                  );
                }

                const option = item as LinkPickerOption;
                const selectableIndex = selectableItems.indexOf(option);
                const isHighlighted = selectableIndex === highlightedIndex;

                return (
                  <button
                    key={option.value}
                    onClick={() => handleSelect(option.value)}
                    onMouseEnter={() => setHighlightedIndex(selectableIndex)}
                    disabled={option.disabled}
                    className={`w-full text-left px-3 py-2 text-sm transition-colors ${
                      isHighlighted ? "bg-primary/10 text-foreground" : "text-foreground hover:bg-muted"
                    } ${option.disabled ? "opacity-50 cursor-not-allowed" : "cursor-pointer"}`}
                  >
                    {option.label}
                  </button>
                );
              })
            )}
          </div>
        </div>
      )}
    </div>
  );
}
