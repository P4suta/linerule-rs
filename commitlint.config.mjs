// Conventional Commits enforcement.
// Strict on subject (case, length, type) — relaxed on body / footer line
// length so bot-authored commits (Dependabot, Renovate) whose generated
// bodies embed long SHAs / URLs are not rejected by mechanical formatting.

export default {
    extends: ['@commitlint/config-conventional'],
    rules: {
        'header-max-length': [2, 'always', 100],
        'header-min-length': [2, 'always', 10],
        'subject-case': [2, 'always', ['lower-case', 'sentence-case']],
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
