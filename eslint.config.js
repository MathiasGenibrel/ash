import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
    {
        // Les worktrees de tâche sont des copies du dépôt : les inclure ferait linter
        // le même fichier autant de fois qu'il y a de tâches en cours.
        ignores: ["dist/", "src-tauri/", ".claude/worktrees/"],
    },
    js.configs.recommended,
    tseslint.configs.recommendedTypeChecked,
    {
        languageOptions: {
            parserOptions: {
                projectService: true,
                tsconfigRootDir: import.meta.dirname,
            },
        },
        rules: {
            // Le contrat IPC franchit la frontière Tauri en JSON : tout ce qui en
            // vient est `unknown` tant qu'il n'a pas été validé, et le laisser filer
            // en `any` annulerait le `strict` du tsconfig.
            "@typescript-eslint/no-unsafe-assignment": "error",
            "@typescript-eslint/no-explicit-any": "error",

            "@typescript-eslint/consistent-type-imports": [
                "error",
                { fixStyle: "inline-type-imports" },
            ],
            "@typescript-eslint/no-unused-vars": [
                "error",
                { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
            ],
        },
    },
    {
        // Les fichiers de configuration ne sont pas couverts par le tsconfig applicatif.
        files: ["*.js"],
        extends: [tseslint.configs.disableTypeChecked],
    },
);
