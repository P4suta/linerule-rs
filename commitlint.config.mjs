// Conventional Commits enforcement.
// Strict on type / non-empty subject / length — relaxed on case so technical
// subjects can mix the lower-case project name (`linerule-rs`) with proper
// nouns (`Rust`, `Direct2D`, `C#`) naturally, and on body / footer length so
// bot-authored commits (Dependabot, Renovate) with long SHAs / URLs pass.

export default {
    extends: ['@commitlint/config-conventional'],
    rules: {
        'header-max-length': [2, 'always', 100],
        'header-min-length': [2, 'always', 10],
        // subject-case intentionally disabled: technical subjects routinely
        // mix the lower-case project name and capitalized proper nouns;
        // neither `lower-case` nor `sentence-case` fits naturally.
        'subject-case': [0],
        'subject-empty': [2, 'never'],
        'subject-full-stop': [2, 'never', '.'],
        'type-empty': [2, 'never'],
        'body-max-line-length': [0],
        'footer-max-line-length': [0],
        'type-enum': [
            2,
            'always',
            [
                'feat',
                'fix',
                'docs',
                'style',
                'refactor',
                'perf',
                'test',
                'build',
                'ci',
                'chore',
                'revert',
            ],
        ],
    },
};
