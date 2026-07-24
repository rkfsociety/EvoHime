import { useEffect, useRef, useState, ChangeEvent, FormEvent } from "react";
import { useModalA11y } from "../hooks/useModalA11y";

export interface SearchResult {
  path: string;
  line: number;
  end_line: number;
  snippet: string;
  score: number;
}

export function SearchModal({
  isOpen,
  onClose,
  onSelectResult,
}: {
  isOpen: boolean;
  onClose: () => void;
  onSelectResult: (result: SearchResult) => void;
}) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const modalRef = useModalA11y<HTMLDivElement>(isOpen, onClose);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isOpen) {
      inputRef.current?.focus();
    }
  }, [isOpen]);

  const handleSearch = async (e: FormEvent) => {
    e.preventDefault();
    if (!query.trim()) {
      setResults([]);
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const response = await fetch(
        `/api/projects/search?q=${encodeURIComponent(query)}&limit=20`
      );
      if (!response.ok) {
        throw new Error("Поиск не удался");
      }
      const data: SearchResult[] = await response.json();
      setResults(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Ошибка поиска");
    } finally {
      setLoading(false);
    }
  };

  const handleInputChange = (e: ChangeEvent<HTMLInputElement>) => {
    setQuery(e.target.value);
  };

  if (!isOpen) {
    return null;
  }

  return (
    <div className="searchBackdrop" role="presentation" onClick={onClose}>
      <div
        ref={modalRef}
        className="searchModal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="search-modal-title"
        tabIndex={-1}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="searchModalHeader">
          <h3 id="search-modal-title">Поиск в проекте</h3>
          <button
            type="button"
            className="searchModalClose"
            onClick={onClose}
            aria-label="Закрыть поиск"
          >
            ✕
          </button>
        </div>

        <form onSubmit={handleSearch} className="searchForm">
          <input
            ref={inputRef}
            type="text"
            className="searchInput"
            value={query}
            onChange={handleInputChange}
            placeholder="Поиск кода, имён файлов..."
            disabled={loading}
          />
          <button
            type="submit"
            className="searchSubmitButton"
            disabled={loading}
          >
            {loading ? "Поиск..." : "Найти"}
          </button>
        </form>

        {error && <div className="searchError">{error}</div>}

        <div className="searchResults">
          {results.length === 0 && query && !loading ? (
            <div className="searchEmpty">Результатов не найдено</div>
          ) : results.length > 0 ? (
            <ul className="searchResultsList">
              {results.map((result, idx) => (
                <li key={idx}>
                  <button
                    type="button"
                    className="searchResultItem"
                    onClick={() => {
                      onSelectResult(result);
                      onClose();
                    }}
                  >
                    <div className="searchResultPath">{result.path}</div>
                    <div className="searchResultLine">
                      Строка {result.line}: {result.snippet.trim().slice(0, 100)}...
                    </div>
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      </div>
    </div>
  );
}
