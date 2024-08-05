const Configuration = {
    extends: ['@commitlint/config-conventional'],
    rules: {
        'type-enum': [2, 'always', [
            'change',
            'chore',
            'ci',
            'deprecated',
            'doc',
            'feat',
            'fix',
            'perf',
            'refactor',
            'revert',
            'style',
            'test',
        ]],
        'header-max-length': [1, 'always', 72],
        'subject-case': [1, 'always', [
            'lower-case', // lower case
            'sentence-case', // Sentence case
            'start-case', // Start Case
        ]],
    },
}

export default Configuration
