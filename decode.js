const orig = "Năm 1866 được đánh dấu bằng một sự kiện kỳ lạ, một hiện tượng không được giải thích và không thể giải thích nổi mà chắc chưa ai quên. Nếu đó là một loại cá voi thì theo sự miêu tả, nó lớn hơn tất cả những con cá voi đã được khoa học biết đến.";
const ext = "Nǔm 1866 àûuác àaánh dêẽu bùçng mõät sủã kiiãn kyã laả, mõät hiiãn tûuản khäng àûuác giaãi thêch vaâ khäng thã giaãi thêch näi maâ chàc chûa ai quãn. Ýêu àoá laå mõät loaåi caá voi thò theo sủå miĩu taã, noá lúán hún tẽêt caã nhûäng con caá voi àaä àûuác khoa hoåc biãêt àãên.";

const origWords = orig.split(' ');
const extWords = ext.split(' ');

for (let i = 0; i < origWords.length; i++) {
  console.log(extWords[i].padEnd(10), '->', origWords[i]);
}
