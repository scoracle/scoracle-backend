// Render installed Ollama Go templates for exact offline token preflight.
package main

import (
	"encoding/json"
	"os"
	"text/template"
	"time"
)

func main() {
	var request struct {
		Template string         `json:"template"`
		Data     map[string]any `json:"data"`
	}
	if err := json.NewDecoder(os.Stdin).Decode(&request); err != nil {
		panic(err)
	}
	funcs := template.FuncMap{
		"currentDate":   func() string { return time.Now().Format("2006-01-02") },
		"yesterdayDate": func() string { return time.Now().AddDate(0, 0, -1).Format("2006-01-02") },
	}
	t, err := template.New("installed-model").Funcs(funcs).Option("missingkey=error").Parse(request.Template)
	if err != nil {
		panic(err)
	}
	if err := t.Execute(os.Stdout, request.Data); err != nil {
		panic(err)
	}
}
