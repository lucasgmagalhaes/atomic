const player = { level: 10, damage: 20 };
let total = 0;
for (let i = 0; i < 1000000; i++) {
    total += player.damage;
}
total;
