function cost(level) {
    if (level < 1) { return 10; }
    return cost(level - 1) * 1.15;
}
function affordableLevels(budget, level) {
    let price = cost(level);
    if (price > budget) { return level; }
    return affordableLevels(budget - price, level + 1);
}
affordableLevels(1000, 0);
