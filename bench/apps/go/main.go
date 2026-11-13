// Go baseline: standard library router (Go 1.22+ patterns) + pgx pool.
package main

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"os"
	"strconv"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

type User struct {
	ID    int64  `json:"id"`
	Name  string `json:"name"`
	Email string `json:"email"`
	Age   int64  `json:"age"`
}

type newUser struct {
	Name  string `json:"name"`
	Email string `json:"email"`
	Age   int64  `json:"age"`
}

func writeJSON(w http.ResponseWriter, v any) {
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(v)
}

func intParam(r *http.Request, name string, def int64) int64 {
	if v, err := strconv.ParseInt(r.URL.Query().Get(name), 10, 64); err == nil {
		return v
	}
	return def
}

func main() {
	ctx := context.Background()
	cfg, err := pgxpool.ParseConfig(os.Getenv("DATABASE_URL"))
	if err != nil {
		panic(err)
	}
	cfg.MaxConns = 32
	if v, err := strconv.Atoi(os.Getenv("DB_POOL")); err == nil {
		cfg.MaxConns = int32(v)
	}
	db, err := pgxpool.NewWithConfig(ctx, cfg)
	if err != nil {
		panic(err)
	}

	mux := http.NewServeMux()
	mux.HandleFunc("GET /ping", func(w http.ResponseWriter, r *http.Request) {
		writeJSON(w, map[string]bool{"ok": true})
	})
	mux.HandleFunc("GET /users/{id}", func(w http.ResponseWriter, r *http.Request) {
		id, err := strconv.ParseInt(r.PathValue("id"), 10, 64)
		if err != nil {
			http.Error(w, "bad id", http.StatusBadRequest)
			return
		}
		var u User
		err = db.QueryRow(r.Context(), "SELECT id, name, email, age FROM users WHERE id = $1", id).
			Scan(&u.ID, &u.Name, &u.Email, &u.Age)
		if errors.Is(err, pgx.ErrNoRows) {
			http.Error(w, "not found", http.StatusNotFound)
			return
		} else if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		writeJSON(w, map[string]User{"user": u})
	})
	mux.HandleFunc("GET /users", func(w http.ResponseWriter, r *http.Request) {
		limit := min(max(intParam(r, "limit", 100), 0), 1000)
		offset := max(intParam(r, "offset", 0), 0)
		rows, err := db.Query(r.Context(), "SELECT id, name, email, age FROM users ORDER BY id LIMIT $1 OFFSET $2", limit, offset)
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		users, err := pgx.CollectRows(rows, func(row pgx.CollectableRow) (User, error) {
			var u User
			err := row.Scan(&u.ID, &u.Name, &u.Email, &u.Age)
			return u, err
		})
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		writeJSON(w, map[string][]User{"users": users})
	})
	mux.HandleFunc("POST /users", func(w http.ResponseWriter, r *http.Request) {
		var in newUser
		if err := json.NewDecoder(r.Body).Decode(&in); err != nil {
			http.Error(w, err.Error(), http.StatusUnprocessableEntity)
			return
		}
		var u User
		err := db.QueryRow(r.Context(),
			"INSERT INTO users (name, email, age) VALUES ($1, $2, $3) RETURNING id, name, email, age",
			in.Name, in.Email, in.Age).Scan(&u.ID, &u.Name, &u.Email, &u.Age)
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		writeJSON(w, map[string]User{"user": u})
	})

	port := os.Getenv("APP_PORT")
	if port == "" {
		port = "8080"
	}
	if err := http.ListenAndServe(":"+port, mux); err != nil {
		panic(err)
	}
}
