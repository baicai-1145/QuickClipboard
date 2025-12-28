function Select({
  value,
  onChange,
  options,
  className = '',
  disabled = false,
  ...rest
}) {
  return <select
      value={value}
      onChange={e => onChange(e.target.value)}
      disabled={disabled}
      className={`px-3 py-2 bg-white dark:bg-gray-700 border border-gray-300 dark:border-gray-600 rounded-lg text-gray-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-blue-500 cursor-pointer ${disabled ? 'opacity-60 cursor-not-allowed' : ''} ${className}`}
      {...rest}
    >
      {options.map(option => <option key={option.value} value={option.value}>
          {option.label}
        </option>)}
    </select>;
}
export default Select;
